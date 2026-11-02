import XCTest

final class PanesAndSettingsTests: FixtureTestCase {
    /// Newest first: the fixture's two switches are newer than both its enrolments.
    @MainActor
    func testActivityListsChanges() {
        let app = launch(.oneTool)
        app.openWindow()
        app.descendants(matching: .any)["sidebar.activity"].click()
        let switched = app.text("Switch")
        let enrolled = app.text("Enrol")
        XCTAssertTrue(switched.waitForExistence(timeout: 5))
        XCTAssertTrue(enrolled.exists)
        XCTAssertLessThan(switched.frame.minY, enrolled.frame.minY, "newest first")
    }

    /// The one thing worth looking at in this fixture is personal's parked login, which
    /// lapses soon.
    @MainActor
    func testThisMacShowsTheChecks() {
        let app = launch(.oneTool)
        app.openWindow()
        app.descendants(matching: .any)["sidebar.machine"].click()
        XCTAssertTrue(app.staticTexts["Account personal"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["One thing is worth looking at."].exists)
    }

    @MainActor
    func testDailyRenewal() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons["Renewal"].click()
        let renew = app.control("settings.renewDaily")
        XCTAssertTrue(renew.waitForExistence(timeout: 5))
        renew.click()
        XCTAssertTrue(app.text("Every day").waitForExistence(timeout: 5))
        app.buttons["Renew Now"].click()
        XCTAssertTrue(app.text("Renewed one.").waitForExistence(timeout: 5))
    }

    /// Opening at login turns on and off without registering the test build.
    @MainActor
    func testOpenAtLogin() {
        let app = launch(.oneTool)
        app.openSettings()
        let toggle = app.control("settings.openAtLogin")
        XCTAssertTrue(toggle.waitForExistence(timeout: 5))
        XCTAssertEqual(toggle.value as? Int, 0)
        toggle.click()
        XCTAssertTrue(toggle.wait(until: "value == 1", timeout: 5))
        toggle.click()
        XCTAssertTrue(toggle.wait(until: "value == 0", timeout: 5))
    }

    /// The model answers a change after the click, so each read after one waits.
    @MainActor
    func testSwitchingBeforeAnAccountRunsOut() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons["Limits"].click()
        let toggle = app.control("settings.autoSwitch")
        let share = app.control("settings.autoSwitchAt")
        let standing = app.control("settings.autoSwitchStanding")
        XCTAssertTrue(toggle.waitForExistence(timeout: 5))
        XCTAssertTrue(share.exists)
        // Both are held back until the app's preferences have been read.
        XCTAssertTrue(toggle.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        XCTAssertEqual(toggle.value as? Int, 0)
        XCTAssertFalse(share.isEnabled, "no share to change while it is off")
        XCTAssertFalse(standing.exists, "nothing said while it is off")

        toggle.click()
        XCTAssertTrue(toggle.wait(until: "value == 1", timeout: 5))
        XCTAssertTrue(share.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        XCTAssertTrue(says(app, "Switch when a limit reaches 95%").waitForExistence(timeout: 5))
        XCTAssertTrue(standing.waitForExistence(timeout: 5), "what the look came to")

        let raise = share.incrementArrows.firstMatch
        XCTAssertTrue(raise.exists, "a stepper shows its arrows")
        raise.click()
        XCTAssertTrue(says(app, "Switch when a limit reaches 96%").waitForExistence(timeout: 5))

        toggle.click()
        XCTAssertTrue(toggle.wait(until: "value == 0", timeout: 5))
        XCTAssertTrue(share.wait(for: \.isEnabled, toEqual: false, timeout: 5))
        XCTAssertTrue(standing.waitForNonExistence(timeout: 5))
    }

    /// Codex/Pi is opt-in and changing it never changes Claude's controls.
    @MainActor
    func testCodexPiHasIndependentAutomaticSwitchControls() {
        let app = launch(.twoTools)
        app.openSettings()
        app.toolbars.buttons["Limits"].click()
        let claude = app.control("settings.autoSwitch")
        let claudeShare = app.control("settings.autoSwitchAt")
        let codex = app.control("settings.codexAutoSwitch")
        let codexShare = app.control("settings.codexAutoSwitchAt")
        XCTAssertTrue(codex.waitForExistence(timeout: 5))
        XCTAssertTrue(codex.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        XCTAssertEqual(codex.value as? Int, 0)
        XCTAssertEqual(claude.value as? Int, 0)
        XCTAssertFalse(codexShare.isEnabled)
        codex.click()
        XCTAssertTrue(codex.wait(until: "value == 1", timeout: 5))
        XCTAssertTrue(codexShare.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        codexShare.incrementArrows.firstMatch.click()
        XCTAssertTrue(says(app, "Switch when a limit reaches 96%").waitForExistence(timeout: 5))
        XCTAssertEqual(claude.value as? Int, 0)
        XCTAssertFalse(claudeShare.isEnabled)
        claude.click()
        XCTAssertTrue(claude.wait(until: "value == 1", timeout: 5))
        XCTAssertTrue(says(app, "Switch when a limit reaches 95%").exists)
        codex.click()
        XCTAssertTrue(codex.wait(until: "value == 0", timeout: 5))
        XCTAssertEqual(claude.value as? Int, 1)
        XCTAssertTrue(claudeShare.isEnabled)
    }

    /// Kept warm-up accounts do not bring the removed section back into Limits.
    @MainActor
    func testLimitsHasNoWarmUpControls() {
        let app = launch(.warmUp)
        app.openSettings()
        app.toolbars.buttons["Limits"].click()
        XCTAssertTrue(app.control("settings.autoSwitch").waitForExistence(timeout: 5))
        for identifier in [
            "settings.warmFrom", "settings.warmTo", "settings.warmStanding",
            "settings.warm.claude/personal", "settings.warm.claude/work",
        ] {
            XCTAssertFalse(app.control(identifier).exists, identifier)
        }
    }

    /// What names the share a limit is switched at, as `words`: a text beside the stepper,
    /// or the stepper itself where macOS reads its label as the stepper's.
    @MainActor
    private func says(_ app: XCUIApplication, _ words: String) -> XCUIElement {
        let format = "label == %@ OR CAST(value, \"NSString\") == %@"
        let named = NSPredicate(format: format, words, words)
        return app.descendants(matching: .any).matching(named).firstMatch
    }

    /// The fixture has no Pitboard of its own, so the tab offers to link the one inside the
    /// app.
    @MainActor
    func testInstallingTheCommandLine() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons["Command Line"].click()
        let install = app.buttons["Install Command Line Tool…"]
        XCTAssertTrue(install.waitForExistence(timeout: 5))
        install.click()
        XCTAssertTrue(install.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.text("ENDSWITH", "/bin/pitboard").waitForExistence(timeout: 5))
    }

    /// A debug build carries no update key, and the Updates tab says so.
    @MainActor
    func testUpdatesSayWhenTheBuildCannotUpdate() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons["Updates"].click()
        let prefix = "This copy of Pitboard can"
        let note = app.staticTexts.matching(
            NSPredicate(format: "value BEGINSWITH %@ OR label BEGINSWITH %@", prefix, prefix)
        ).firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 5))
    }
}
