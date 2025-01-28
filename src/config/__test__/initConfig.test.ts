import fs from "fs";
import { initConfig } from "../initConfig";
import {
  defaultConfigDir,
  defaultConfigPath,
  defaultConfigYml,
} from "../defaultConfigYml";

describe("initConfig", () => {
  it("writes a default config at the default location, creating parent folders as needed", () => {
    // TODO: Use mocks to better insulate test from filesystem and real default config
    const writeFileSpy = jest
      .spyOn(fs, "writeFileSync")
      .mockImplementation(() => {});
    const mkDirSpy = jest.spyOn(fs, "mkdirSync").mockImplementation(() => {
      return undefined;
    });

    initConfig();

    expect(mkDirSpy).toHaveBeenCalledWith(defaultConfigDir + "/datum", {
      recursive: true,
    });
    expect(writeFileSpy).toHaveBeenCalledWith(
      defaultConfigPath,
      defaultConfigYml.toString(),
      {
        encoding: "utf-8",
      },
    );
  });
});
