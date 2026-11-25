import { describe, expect, it } from "vitest";
import type { Installation } from "./api";
import { launcherAction, versionExplanation } from "./games";

const game = (over: Partial<Installation>): Installation => ({
  id: 1,
  source: "steam",
  sourceGameId: "10",
  name: "Game",
  path: "/games/game",
  installedVersion: null,
  versionSource: null,
  versionConfidence: "unknown",
  versionConflict: false,
  launcherVersion: null,
  exeVersion: null,
  userVersion: null,
  updateAvailable: null,
  sizeBytes: null,
  lastUpdated: null,
  scannedAt: 0,
  launcher: "steam",
  dlc: [],
  ...over,
});

describe("launcherAction", () => {
  it("opens Steam's library page for the app", () => {
    expect(launcherAction(game({}))?.url).toBe("steam://nav/games/details/10");
  });

  it("encodes ids read from disk so they cannot inject URL parts", () => {
    expect(launcherAction(game({ sourceGameId: "10/../run?x=1" }))?.url).toBe(
      "steam://nav/games/details/10%2F..%2Frun%3Fx%3D1",
    );
  });

  it("routes by the managing launcher, not the store", () => {
    expect(launcherAction(game({ source: "gog", launcher: "galaxy" }))?.url).toBe("goggalaxy://openGameView/10");
    expect(launcherAction(game({ source: "epic", launcher: "epic" }))?.url).toMatch(/^com\.epicgames\.launcher:\/\/apps\/10/);
    // Store is GOG or Epic, but Heroic/Legendary manage the install: never hand it to Galaxy or Epic.
    expect(launcherAction(game({ source: "gog", launcher: "heroic" }))).toBeNull();
    expect(launcherAction(game({ source: "epic", launcher: "legendary" }))).toBeNull();
    expect(launcherAction(game({ source: "gog", launcher: null }))).toBeNull();
  });

  it("offers nothing for unknown launchers", () => {
    expect(launcherAction(game({ launcher: "other" }))).toBeNull();
  });
});

describe("versionExplanation", () => {
  it("says unknown instead of guessing", () => {
    expect(versionExplanation(game({}))).toMatch(/Unknown rather than guessing/);
  });

  it("names the source and confidence", () => {
    expect(
      versionExplanation(game({ installedVersion: "5", versionSource: "steam_buildid", versionConfidence: "launcher" })),
    ).toMatch(/Steam build ID.*Confidence: launcher metadata/);
  });

  it("warns that an executable's version may not be the game's", () => {
    const g = game({ installedVersion: "1.2", versionSource: "executable", versionConfidence: "executable" });
    expect(versionExplanation(g)).toMatch(/executable.*not always the full game version/);
  });

  it("points at conflicts", () => {
    const g = game({ installedVersion: "1.3", versionSource: "user", versionConfidence: "user", versionConflict: true });
    expect(versionExplanation(g)).toMatch(/entered by you.*Sources disagree/);
  });
});
