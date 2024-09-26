import { loadConfig } from "../loadConfig";

const configFile = `${__dirname}/__fixtures__/test_datumrc.yml`;

describe("loadConfig", () => {
  it("should load a config file", () => {
    const args = {
      configFile,
    };
    const config = loadConfig(args);
    expect(config).toEqual({
      project_dir: process.env["HOME"] + "/some/project/dir/path",
      db: "test_db",
      connection: {
        host: process.env["XDG_DATA_HOME"] + "/test_host",
        user: "test_user",
        password: "test_password",
      },
    });
  });

  it("should throw an error if the config file doesn't exist", () => {
    const args = {
      configFile: "/test/fixtures/nonexistent.yml",
    };
    expect(() => loadConfig(args)).toThrow(
      "Config file not found: /test/fixtures/nonexistent.yml",
    );
  });

  it("should throw an error if the default config file doesn't exist and no file was specified", () => {
    process.env["XDG_CONFIG_HOME"] = __dirname;
    const args = {};
    expect(() => loadConfig(args)).toThrow(
      "Datum config file not found. Please run 'datum init' to create one.",
    );
  });

  it("should replace environment variables in the config", () => {
    process.env["HOME"] = "/home/test";
    process.env["XDG_DATA_HOME"] = "/home/test/.local/share";
    const args = {
      configFile,
    };
    const config = loadConfig(args);
    expect(config).toEqual({
      project_dir: "/home/test/some/project/dir/path",
      db: "test_db",
      connection: {
        host: "/home/test/.local/share/test_host",
        user: "test_user",
        password: "test_password",
      },
    });
  });
});
