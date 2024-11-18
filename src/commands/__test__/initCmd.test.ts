import fs from "fs";
import { initCmd } from "../initCmd";
import { pass } from "../../__test__/test-utils";
import yaml from "yaml";
import { defaultConfigYml } from "../../config/defaultConfigYml";

describe("initCmd", () => {
  beforeEach(() => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    delete process.env.COUCHDB_HOST;
    delete process.env.COUCHDB_USER;
    delete process.env.COUCHDB_PASSWORD;

    try {
      fs.rmSync("/tmp/datum/datumrc.yml");
    } catch (e) {
      pass;
    }

    // initConfig uses fetch to query if couchdb is running, so mock it
    jest
      .spyOn(global, "fetch")
      .mockImplementation(() => Promise.reject(new Error("url not found")));
  });

  it("writes datumrc into the xdg_config directory", async () => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    await initCmd({ nonInteractive: true });
    fs.accessSync("/tmp/datum/datumrc.yml");
    const configFile = fs.readFileSync("/tmp/datum/datumrc.yml", "utf-8");
    expect(configFile).toEqual(defaultConfigYml);
  });

  it("fails if datumrc exists and --overwrite is not specified", async () => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    fs.writeFileSync("/tmp/datum/datumrc.yml", "test");
    await expect(initCmd({ nonInteractive: true })).rejects.toThrow();
  });

  it("overwrites datumrc if it exists and --overwrite is specified", async () => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    fs.writeFileSync("/tmp/datum/datumrc.yml", "test");
    await initCmd({ nonInteractive: true, overwrite: true });
    fs.accessSync("/tmp/datum/datumrc.yml");
    const configFile = fs.readFileSync("/tmp/datum/datumrc.yml", "utf-8");
    expect(configFile).toEqual(defaultConfigYml);
  });

  it("uses environment variables if they are set", async () => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    process.env.COUCHDB_HOST = "http://localhost:5983";
    process.env.COUCHDB_USER = "env_user";

    await initCmd({ nonInteractive: true });
    fs.accessSync("/tmp/datum/datumrc.yml");
    const configFile = fs.readFileSync("/tmp/datum/datumrc.yml", "utf-8");
    const config = yaml.parseDocument(configFile);
    expect(config.getIn(["connection", "host"])).toEqual(
      process.env.COUCHDB_HOST,
    );
    expect(config.getIn(["connection", "user"])).toEqual(
      process.env.COUCHDB_USER,
    );
  });

  it("uses direct arguments with precendence even over environment variables", async () => {
    process.env.XDG_CONFIG_HOME = "/tmp";
    process.env.COUCHDB_HOST = "http://localhost:5983";
    process.env.COUCHDB_USER = "env_user";

    await initCmd({
      nonInteractive: true,
      host: "http://localhost:5984",
      user: "arg_user",
    });
    fs.accessSync("/tmp/datum/datumrc.yml");
    const configFile = fs.readFileSync("/tmp/datum/datumrc.yml", "utf-8");
    const config = yaml.parseDocument(configFile);
    expect(config.getIn(["connection", "host"])).toEqual(
      "http://localhost:5984",
    );
    expect(config.getIn(["connection", "user"])).toEqual("arg_user");
  });
});
