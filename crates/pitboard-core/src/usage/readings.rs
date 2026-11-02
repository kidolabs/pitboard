//! The newest usage reading Pitboard knows for each account.
//!
//! When an account cannot be asked, because Anthropic is unreachable or its parked login
//! could not be renewed, the only honest thing to show is the last number actually measured,
//! and when.
//!
//! Every front end records here and shows what is here: a status line in each open session,
//! the command line and the menu bar. A session knows only what its own last response said,
//! so when each showed its own numbers and whoever wrote last won, sessions on one account
//! disagreed with each other and with the menu bar. An answer from the service sets an
//! account's reading, and a session's numbers only move a limit an answer gave (see
//! [`crate::usage::answered_for`] and [`crate::usage::moved`]), so a reading is the newest thing
//! any of them has seen of that account. Nothing here is secret.

use crate::context::Context;
use crate::provider::ProviderId;
use crate::service::Permit;
use crate::usage::{Snapshot, Source, Window};
use crate::{atomic, home};
use std::collections::HashMap;
use std::path::PathBuf;

fn path(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("usage.json")
}

/// Private disk envelope: public Snapshot and its JSON shape stay unchanged. An older
/// writer deserializes only Snapshot, drops this marker, and therefore fails closed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Record {
    #[serde(flatten)]
    snapshot: Snapshot,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    codex_raw_windows: bool,
}

fn records(ctx: &Context) -> HashMap<String, Record> {
    std::fs::read_to_string(path(ctx))
        .ok()
        .and_then(|raw| serde_json::from_str::<HashMap<String, Record>>(&raw).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|(uuid, mut record)| {
            record.snapshot.source = Source::Remembered;
            (uuid, record)
        })
        .collect()
}

pub(crate) fn load(ctx: &Context) -> HashMap<String, Snapshot> {
    records(ctx)
        .into_iter()
        .map(|(id, record)| (id, record.snapshot))
        .collect()
}

/// Only readings founded on an accepted provider-aware raw Codex answer. Return the
/// snapshot too, so a judge cannot attach provenance to a different concurrent reading.
pub(crate) fn codex_raw(ctx: &Context) -> HashMap<String, Snapshot> {
    records(ctx)
        .into_iter()
        .filter(|(_, record)| record.codex_raw_windows)
        .map(|(id, record)| (id, record.snapshot))
        .collect()
}

/// When the readings last changed, in epoch milliseconds, or 0 when there are none.
///
/// For a front end to follow what the others record without asking anyone. Milliseconds
/// rather than seconds, because sessions record moments apart, and the second of two
/// changes within one second would otherwise go unseen until the next.
pub(crate) fn changed_at(ctx: &Context) -> i64 {
    std::fs::metadata(path(ctx))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// Drop what was remembered for an account that is no longer enrolled. Nothing here is
/// secret, but an account someone has dropped should leave no trace behind either.
pub(crate) fn forget(ctx: &Context, permit: Permit, id: &str) {
    change(ctx, permit, |all| all.remove(id).is_some());
}

/// The existing Claude fixture helper. Production writers must name the actual provider.
#[cfg(test)]
pub(crate) fn answered(ctx: &Context, permit: Permit, answers: &[(String, Snapshot)]) {
    answered_for(
        ctx,
        permit,
        &answers
            .iter()
            .map(|(id, answer)| (ProviderId::Claude, id.clone(), answer.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Raw service answers, each folded under the usage lock with its actual provider.
/// Only an accepted new Codex answer can mint provenance; stale answers preserve what
/// was already known, and Claude answers clear any Codex claim for the affected account.
pub(crate) fn answered_for(
    ctx: &Context,
    permit: Permit,
    answers: &[(ProviderId, String, Snapshot)],
) {
    let now = ctx.now();
    change(ctx, permit, |all| {
        let mut changed = false;
        for (provider, id, answer) in answers {
            let known = all.get(id);
            let snapshot = known.map(|record| &record.snapshot);
            if !crate::usage::accepts_answer(snapshot, answer, now) {
                continue;
            }
            let mut next = crate::usage::answered_for(*provider, snapshot, answer, now);
            // What it is once stored, as `load` says of everything here.
            next.source = Source::Remembered;
            let next = Record {
                snapshot: next,
                codex_raw_windows: *provider == ProviderId::Codex && answer.answered(),
            };
            if known != Some(&next) {
                all.insert(id.clone(), next);
                changed = true;
            }
        }
        changed
    });
}

/// What a session on account `id` passed its status line, recorded where it moves a limit
/// of that account's reading, as [`crate::usage::moved`] says. An account with no reading
/// gets none from it.
pub(crate) fn moved(ctx: &Context, permit: Permit, id: &str, passed: &[Window]) {
    let now = ctx.now();
    change(ctx, permit, |all| {
        let Some(next) = all.get(id).and_then(|known| {
            crate::usage::moved(&known.snapshot, passed, now).map(|snapshot| Record {
                snapshot,
                codex_raw_windows: known.codex_raw_windows,
            })
        }) else {
            return false;
        };
        all.insert(id.to_owned(), next);
        true
    });
}

/// The readings changed by `fold`, which says whether it changed anything.
///
/// Written only when it did: a status line runs in every open session, as often as every
/// second, and mostly passes what is already here. A change is made under a lock, to what
/// is here once the lock is held, so two front ends writing at once cannot each write back
/// the other's older copy.
fn change(ctx: &Context, permit: Permit, fold: impl Fn(&mut HashMap<String, Record>) -> bool) {
    if !fold(&mut records(ctx)) {
        return;
    }
    let Some(_held) = exclusive(ctx, permit) else {
        return;
    };
    let mut all = records(ctx);
    if fold(&mut all)
        && let Ok(body) = serde_json::to_string(&all)
    {
        let _ = atomic::write(permit, &path(ctx), body.as_bytes(), atomic::Perms::Secret);
    }
}

/// Held while a change is written, here or to what sessions passed their status line. A
/// kernel lock, which the system lets go of when the process ends, and apart from the one
/// around switches: a status line must never wait on a switch.
pub(crate) fn exclusive(ctx: &Context, permit: Permit) -> Option<std::fs::File> {
    home::ensure(ctx, permit).ok()?;
    let file =
        crate::host::fs::open_private_lock(permit, &home::dir(ctx).join("usage.lock")).ok()?;
    file.lock().ok()?;
    Some(file)
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::time::{Clock, FixedClock};
    use std::sync::Arc;

    const NOW: i64 = 1_789_935_000;
    const RESETS: i64 = NOW + 3_600;

    fn machine() -> (Context, tempfile::TempDir) {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();
        let ctx = Context::new(root.clone())
            .with_pitboard_home(root)
            .with_clock(Arc::new(FixedClock::at(NOW)) as Arc<dyn Clock>);
        home::ensure(&ctx, Permit::for_a_test()).expect("a home");
        (ctx, scratch)
    }

    fn window(kind: &str, percent: f64) -> Window {
        Window {
            kind: kind.into(),
            scope: None,
            percent,
            resets_at: Some(RESETS),
            is_active: true,
            severity: None,
            length_seconds: crate::usage::anthropic_window_length(kind),
        }
    }

    /// Anthropic's answer about an account with one limit, taken at `at`.
    fn answer(kind: &str, percent: f64, at: i64) -> Snapshot {
        Snapshot {
            windows: vec![window(kind, percent)],
            observed_at: Some(at),
            answered_at: Some(at),
            lists_every_limit: true,
            source: Source::Live,
        }
    }

    /// A session on `work` passing its status line `percent` of the limit Claude Code calls
    /// `kind`.
    fn passes(ctx: &Context, kind: &str, percent: f64) {
        moved(ctx, Permit::for_a_test(), "work", &[window(kind, percent)]);
    }

    fn five_hour(ctx: &Context, uuid: &str) -> Option<f64> {
        Some(load(ctx).get(uuid)?.windows.first()?.percent)
    }

    /// The readings as they are, laid out as nothing Pitboard writes would lay them out, so
    /// a rewrite shows.
    fn laid_out(ctx: &Context) -> String {
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path(ctx)).unwrap()).unwrap();
        let laid_out = serde_json::to_string_pretty(&written).unwrap();
        std::fs::write(path(ctx), &laid_out).unwrap();
        laid_out
    }

    /// 0.9.0 filed a session's numbers and Claude Code's usage cache under the account its
    /// config named, which could be another login's, and kept a limit until its reset
    /// whatever an answer said. The account's first answer is all that is left of it.
    #[test]
    fn a_reading_0_9_0_wrote_under_the_wrong_account_is_gone_at_the_first_answer() {
        let (ctx, _scratch) = machine();
        std::fs::write(
            path(&ctx),
            serde_json::json!({"work": {
                "windows": [
                    {"kind": "session", "scope": null, "percent": 40.0, "resets_at": RESETS,
                     "is_active": true, "severity": null, "length_seconds": 18000},
                    {"kind": "weekly_all", "scope": null, "percent": 100.0,
                     "resets_at": NOW + 5 * 86_400, "is_active": false, "severity": null,
                     "length_seconds": 604800}
                ],
                "observed_at": NOW - 600,
                "account_uuid": "work",
                "source": "remembered"
            }})
            .to_string(),
        )
        .unwrap();
        assert_eq!(load(&ctx)["work"].answered_at, None, "as 0.9.0 wrote it");

        answered(
            &ctx,
            Permit::for_a_test(),
            &[("work".into(), answer("session", 12.0, NOW))],
        );
        let work = &load(&ctx)["work"];
        let kinds: Vec<&str> = work.windows.iter().map(|w| w.kind.as_str()).collect();
        assert_eq!(kinds, ["session"]);
        assert_eq!(work.answered_at, Some(NOW));
    }

    /// One file holds both tools' readings; only Claude's incomplete answers may retain
    /// omitted running windows. Providers are passed explicitly, not read from row names.
    #[test]
    fn each_answer_is_recorded_by_its_provider() {
        let (ctx, _scratch) = machine();
        let given = |kinds: &[&str], at: i64, lists_every_limit: bool| Snapshot {
            windows: kinds.iter().map(|kind| window(kind, 30.0)).collect(),
            observed_at: Some(at),
            answered_at: Some(at),
            lists_every_limit,
            source: Source::Live,
        };
        for (at, kinds) in [
            (NOW - 60, &["five_hour", "seven_day"][..]),
            (NOW, &["seven_day"]),
        ] {
            answered_for(
                &ctx,
                Permit::for_a_test(),
                &[
                    (ProviderId::Claude, "claude".into(), given(kinds, at, false)),
                    (ProviderId::Codex, "codex".into(), given(kinds, at, false)),
                ],
            );
        }
        let kinds = |id: &str| -> Vec<String> {
            load(&ctx)[id]
                .windows
                .iter()
                .map(|w| w.kind.clone())
                .collect()
        };
        assert_eq!(kinds("claude"), ["five_hour", "seven_day"]);
        assert_eq!(kinds("codex"), ["seven_day"]);
        assert!(!load(&ctx)["codex"].lists_every_limit);
        assert!(codex_raw(&ctx).contains_key("codex"));
        assert!(!codex_raw(&ctx).contains_key("claude"));
    }

    #[test]
    fn stale_codex_answers_cannot_bless_a_legacy_cache() {
        let (ctx, _scratch) = machine();
        let legacy = answer("five_hour", 10.0, NOW);
        std::fs::write(
            path(&ctx),
            serde_json::to_vec(&HashMap::from([("work", legacy.clone())])).unwrap(),
        )
        .unwrap();
        let before = std::fs::read(path(&ctx)).unwrap();
        assert!(codex_raw(&ctx).is_empty());
        answered_for(
            &ctx,
            Permit::for_a_test(),
            &[(
                ProviderId::Codex,
                "work".into(),
                answer("seven_day", 20.0, NOW - 1),
            )],
        );
        assert_eq!(
            std::fs::read(path(&ctx)).unwrap(),
            before,
            "an ignored answer changes nothing"
        );
        assert!(
            codex_raw(&ctx).is_empty(),
            "starting a new reader establishes no provenance"
        );

        answered_for(
            &ctx,
            Permit::for_a_test(),
            &[(ProviderId::Codex, "work".into(), legacy)],
        );
        let trusted = std::fs::read(path(&ctx)).unwrap();
        assert!(
            codex_raw(&ctx).contains_key("work"),
            "an accepted answer can mint it even if shares are unchanged"
        );
        answered_for(
            &ctx,
            Permit::for_a_test(),
            &[(
                ProviderId::Codex,
                "work".into(),
                answer("seven_day", 20.0, NOW - 1),
            )],
        );
        assert_eq!(std::fs::read(path(&ctx)).unwrap(), trusted);
        assert!(
            codex_raw(&ctx).contains_key("work"),
            "a stale answer also cannot revoke prior provenance"
        );
    }

    #[test]
    fn raw_provenance_round_trips_survives_other_writes_and_old_writers_lose_it() {
        let (ctx, _scratch) = machine();
        answered_for(
            &ctx,
            Permit::for_a_test(),
            &[(
                ProviderId::Codex,
                "work".into(),
                answer("five_hour", 10.0, NOW - 60),
            )],
        );
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path(&ctx)).unwrap()).unwrap();
        assert_eq!(saved["work"]["codex_raw_windows"], true);
        let public = serde_json::to_value(&load(&ctx)["work"]).unwrap();
        assert!(
            public.get("codex_raw_windows").is_none(),
            "public Snapshot JSON is unchanged"
        );

        answered(
            &ctx,
            Permit::for_a_test(),
            &[("claude".into(), answer("session", 20.0, NOW))],
        );
        answered_for(
            &ctx,
            Permit::for_a_test(),
            &[(
                ProviderId::Codex,
                "other".into(),
                answer("seven_day", 30.0, NOW),
            )],
        );
        passes(&ctx, "five_hour", 15.0);
        assert_eq!(codex_raw(&ctx)["work"].windows[0].percent, 15.0);
        assert_eq!(codex_raw(&ctx)["work"].answered_at, Some(NOW - 60));
        assert_eq!(codex_raw(&ctx)["work"].observed_at, Some(NOW));
        forget(&ctx, Permit::for_a_test(), "other");
        assert!(codex_raw(&ctx).contains_key("work"));
        assert!(!codex_raw(&ctx).contains_key("claude"));
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path(&ctx)).unwrap()).unwrap();
        assert!(
            saved["claude"].get("codex_raw_windows").is_none(),
            "false markers are not serialized"
        );

        // Exactly what an old reader/writer does: unknown fields disappear on write.
        let old_reader: HashMap<String, Snapshot> =
            serde_json::from_slice(&std::fs::read(path(&ctx)).unwrap()).unwrap();
        std::fs::write(path(&ctx), serde_json::to_vec(&old_reader).unwrap()).unwrap();
        assert_eq!(load(&ctx), old_reader);
        assert!(
            codex_raw(&ctx).is_empty(),
            "marker loss fails closed on a fresh load"
        );
    }

    #[test]
    fn only_accepted_answered_codex_writes_mint_the_marker() {
        let (ctx, _scratch) = machine();
        let mut given = answer("five_hour", 10.0, NOW);
        given.answered_at = None;
        answered_for(
            &ctx,
            Permit::for_a_test(),
            &[(ProviderId::Codex, "work".into(), given)],
        );
        assert!(codex_raw(&ctx).is_empty());
        let given = answer("five_hour", 10.0, NOW);
        answered_for(
            &ctx,
            Permit::for_a_test(),
            &[(ProviderId::Codex, "work".into(), given.clone())],
        );
        assert!(codex_raw(&ctx).contains_key("work"));
        answered(&ctx, Permit::for_a_test(), &[("work".into(), given)]);
        assert!(
            codex_raw(&ctx).is_empty(),
            "a Claude/default write cannot vouch for raw Codex windows"
        );
    }

    /// The owner's panes: a busy one had recorded 22% when an idle one, still holding the
    /// 20% of its last response, ran after it. Whoever writes last, the 22% stands.
    #[test]
    fn a_session_never_moves_a_reading_back_whoever_writes_last() {
        let (ctx, _scratch) = machine();
        answered(
            &ctx,
            Permit::for_a_test(),
            &[("work".into(), answer("session", 22.0, NOW - 60))],
        );
        passes(&ctx, "five_hour", 20.0);
        assert_eq!(five_hour(&ctx, "work"), Some(22.0));
        assert_eq!(load(&ctx)["work"].observed_at, Some(NOW - 60));

        passes(&ctx, "five_hour", 25.0);
        assert_eq!(
            five_hour(&ctx, "work"),
            Some(25.0),
            "and forwards is forwards"
        );
        let work = &load(&ctx)["work"];
        assert_eq!(work.observed_at, Some(NOW));
        assert_eq!(work.answered_at, Some(NOW - 60), "and no answer since");
    }

    /// A session's numbers do not say whose they are, and an account Pitboard has never had
    /// an answer about has no window to know them by.
    #[test]
    fn a_session_founds_no_reading() {
        let (ctx, _scratch) = machine();
        passes(&ctx, "five_hour", 22.0);
        assert!(load(&ctx).is_empty());
        assert!(!path(&ctx).exists());
    }

    /// A status line runs in every open session, as often as every second, and what it
    /// passes is mostly what is already here.
    #[test]
    fn a_reading_that_changes_nothing_leaves_the_file_alone() {
        let (ctx, _scratch) = machine();
        let answers = [
            ("work".into(), answer("session", 22.0, NOW - 60)),
            ("personal".into(), answer("session", 3.0, NOW - 60)),
        ];
        answered(&ctx, Permit::for_a_test(), &answers);
        let laid_out = laid_out(&ctx);

        passes(&ctx, "five_hour", 20.0);
        passes(&ctx, "five_hour", 22.0);
        answered(&ctx, Permit::for_a_test(), &answers);
        assert_eq!(std::fs::read_to_string(path(&ctx)).unwrap(), laid_out);
    }

    /// After a banked reset on claude.ai, this machine's sessions passed 14% of a weekly limit
    /// recorded at 100%, with the same reset. A session cannot say its number is the newer,
    /// so the 100% stands until Pitboard's next answer from Anthropic, which can. From there
    /// sessions move the lower share forward again.
    #[test]
    fn a_banked_reset_is_recorded_by_the_next_answer_and_followed_by_sessions() {
        let (ctx, _scratch) = machine();
        let weekly = |ctx: &Context| load(ctx)["work"].windows[0].percent;
        answered(
            &ctx,
            Permit::for_a_test(),
            &[("work".into(), answer("weekly_all", 100.0, NOW - 3_600))],
        );

        passes(&ctx, "seven_day", 14.0);
        assert_eq!(weekly(&ctx), 100.0, "a session says no time");

        answered(
            &ctx,
            Permit::for_a_test(),
            &[("work".into(), answer("weekly_all", 14.0, NOW))],
        );
        assert_eq!(weekly(&ctx), 14.0);

        passes(&ctx, "seven_day", 15.0);
        assert_eq!(weekly(&ctx), 15.0);
    }

    /// A parked account that ran out, asked about once its window has reset: the answer that
    /// finds nothing used is recorded once, and every read after it that finds the same
    /// leaves the file alone.
    #[test]
    fn a_window_past_its_reset_is_recorded_reset_once() {
        let (ctx, _scratch) = machine();
        let mut full = answer("session", 100.0, NOW - 7_200);
        full.windows[0].resets_at = Some(NOW - 3_600);
        answered(&ctx, Permit::for_a_test(), &[("parked".into(), full)]);

        let mut idle = answer("session", 0.0, NOW);
        idle.windows[0].resets_at = None;
        answered(
            &ctx,
            Permit::for_a_test(),
            &[("parked".into(), idle.clone())],
        );
        assert_eq!(five_hour(&ctx, "parked"), Some(0.0));

        let laid_out = laid_out(&ctx);
        answered(&ctx, Permit::for_a_test(), &[("parked".into(), idle)]);
        assert_eq!(std::fs::read_to_string(path(&ctx)).unwrap(), laid_out);
    }
}
