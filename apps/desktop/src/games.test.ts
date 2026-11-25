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
  updateAvailable: null,
  sizeBytes: null,
  lastUpdated: null,
  scannedAt: 0,
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

  it("never sends Legendary/Heroic installs to the Epic Games Launcher", () => {
    expect(launcherAction(game({ source: "epic", versionSource: "legendary" }))).toBeNull();
  });

  it("offers nothing for unknown sources", () => {
    expect(launcherAction(game({ source: "other" }))).toBeNull();
  });
});

describe("versionExplanation", () => {
  it("says unknown instead of guessing", () => {
    expect(versionExplanation(game({}))).toMatch(/Unknown rather than guessing/);
  });

  it("names the source and confidence", () => {
    expect(versionExplanation(game({ installedVersion: "5", versionSource: "steam_buildid" }))).toMatch(
      /Steam build ID.*Confidence: launcher metadata/,
    );
  });
});
