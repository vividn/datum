import fs from "fs";
import { initCmd } from "../initCmd";

const defaultConfigFile = fs.readFileSync(`${__dirname}/../../config/defaultConfig.yml`;

describe("initCmd", () => {
  it("writes datumrc into the xdg_config directory", async () => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    await initCmd({});
    fs.accessSync("/tmp/datum/datumrc.yml");
    const configFile = fs.readFileSync("/tmp/datum/datumrc.yml", "utf-8");
    expect(configFile).toEqual(defaultConfigFile)
  });

  it("fails if datumrc exists and --overwrite is not specified", async () => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    fs.writeFileSync("/tmp/datum/datumrc.yml", "test");
    await expect(initCmd({})).rejects.toThrow();
  });

  it("overwrites datumrc if it exists and --overwrite is specified", async () => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    fs.writeFileSync("/tmp/datum/datumrc.yml", "test");
    await initCmd({ overwrite: true });
    fs.accessSync("/tmp/datum/datumrc.yml");
    const configFile = fs.readFileSync("/tmp/datum/datumrc.yml", "utf-8");
    expect(configFile).toEqual(defaultConfigFile)
  });
});
