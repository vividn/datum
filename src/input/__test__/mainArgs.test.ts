import { datum } from "../mainArgs";
import { topLevelHelpParser } from "../topLevelHelp";
import { getCmd } from "../../commands/getCmd";
import { endCmd } from "../../commands/endCmd";
import { updateCmd } from "../../commands/updateCmd";
import { startCmd } from "../../commands/startCmd";
import { setupCmd } from "../../commands/setupCmd";
import { migrateCmd } from "../../commands/migrateCmd";

// Mock the necessary modules
jest.mock("../topLevelHelp", () => ({
  topLevelHelpParser: {
    print_help: jest.fn(),
  },
}));

jest.mock("../../commands/getCmd", () => ({
  getCmd: jest.fn().mockResolvedValue({}),
}));

jest.mock("../../commands/endCmd", () => ({
  endCmd: jest.fn().mockResolvedValue({}),
}));

jest.mock("../../commands/updateCmd", () => ({
  updateCmd: jest.fn().mockResolvedValue({}),
}));

jest.mock("../../commands/startCmd", () => ({
  startCmd: jest.fn().mockResolvedValue({}),
}));

jest.mock("../../commands/setupCmd", () => ({
  setupCmd: jest.fn().mockResolvedValue({}),
}));

jest.mock("../../commands/migrateCmd", () => ({
  migrateCmd: jest.fn().mockResolvedValue({}),
}));

describe("mainArgs", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  describe("Command line string parsing", () => {
    it("should parse a string command into arguments", async () => {
      await datum("get some-id");
      expect(getCmd).toHaveBeenCalledWith(["some-id"], expect.anything());
    });

    it("should handle quoted arguments in string commands", async () => {
      await datum('get "complex id with spaces"');
      expect(getCmd).toHaveBeenCalledWith(
        ["complex id with spaces"],
        expect.anything(),
      );
    });

    it("should properly escape special characters in string commands", async () => {
      // Note: shell-quote behavior is different than expected here
      // It splits 'id\ with\ backslashes' into ['id', 'with', 'backslashes']
      await datum('get "id with\\ backslashes"');
      expect(getCmd).toHaveBeenCalledWith(
        ["id with\\ backslashes"],
        expect.anything(),
      );
    });
  });

  describe("Help messages", () => {
    it("should display the top level help message when no command is given", async () => {
      await datum([]);
      expect(topLevelHelpParser.print_help).toHaveBeenCalled();
    });

    it("should display the top level help message when an empty string is given", async () => {
      await datum("");
      expect(topLevelHelpParser.print_help).toHaveBeenCalled();
    });

    it("should display the top level help message when an unknown command is given and report an error", async () => {
      await expect(datum(["unknown-command"])).rejects.toThrow(
        'command "unknown-command" not recognized',
      );
      expect(topLevelHelpParser.print_help).toHaveBeenCalled();
    });

    it("should display the top level help message when the --help flag is given", async () => {
      await datum(["--help"]);
      expect(topLevelHelpParser.print_help).toHaveBeenCalled();
    });

    it("should display the top level help message with the help command", async () => {
      await datum(["help"]);
      expect(topLevelHelpParser.print_help).toHaveBeenCalled();
    });

    it("should display a subcommand's help message if the command is given with --help", async () => {
      await datum(["get", "--help"]);
      expect(getCmd).toHaveBeenCalledWith(["--help"], expect.anything());

      await datum("start --help");
      expect(startCmd).toHaveBeenCalledWith(["--help"], expect.anything());
    });

    it("should display help even with database arguments present", async () => {
      await datum(["--db", "testdb", "--host", "localhost:5984", "help"]);
      expect(topLevelHelpParser.print_help).toHaveBeenCalled();
    });
  });

  describe("Command parsing and selection", () => {
    it("should run a command when a valid command is given", async () => {
      await datum(["get", "some-id"]);
      expect(getCmd).toHaveBeenCalledWith(["some-id"], expect.anything());
    });

    it("should handle 'stop' as an alias for 'end' command", async () => {
      await datum(["stop", "some-arg"]);
      expect(endCmd).toHaveBeenCalledWith(["some-arg"], expect.anything());
    });

    it("should handle command aliases correctly", async () => {
      // Testing updateCmd aliases
      await datum(["merge", "id", "data=value"]); // uses updateCmd with strategy=merge
      expect(updateCmd).toHaveBeenCalledWith(
        ["id", "data=value"],
        expect.objectContaining({ strategy: "merge" }),
      );

      await datum(["rekey", "old-id", "new-id"]); // uses updateCmd with strategy=rekey
      expect(updateCmd).toHaveBeenCalledWith(
        ["old-id", "new-id"],
        expect.objectContaining({ strategy: "rekey" }),
      );

      // Testing migrate aliases
      await datum(["mig", "run", "some-migration"]); // alias for migrate
      expect(migrateCmd).toHaveBeenCalledWith(
        ["run", "some-migration"],
        expect.anything(),
      );

      await datum(["migration", "run", "some-migration"]); // another alias for migrate
      expect(migrateCmd).toHaveBeenCalledWith(
        ["run", "some-migration"],
        expect.anything(),
      );
    });

    it("should handle database arguments alongside commands", async () => {
      await datum(["--db", "testdb", "get", "some-id"]);
      expect(getCmd).toHaveBeenCalledWith(
        ["some-id"],
        expect.objectContaining({ db: "testdb" }),
      );

      await datum("--host localhost:8080 setup --view");
      expect(setupCmd).toHaveBeenCalledWith(
        ["--view"],
        expect.objectContaining({ host: "localhost:8080" }),
      );
    });

    it("should set show property to the appropriate value", async () => {
      await datum(["get", "some-id"]);
      // Check that namespace.show is set to default when called from command line
      expect(getCmd).toHaveBeenCalledWith(
        ["some-id"],
        expect.objectContaining({ show: "default" }),
      );

      // Special case for setup command, which sets show to minimal by default
      await datum(["setup"]);
      expect(setupCmd).toHaveBeenCalledWith(
        [],
        expect.objectContaining({ show: "minimal" }),
      );
    });

    it("should set strategy property for update command variants", async () => {
      await datum(["update", "some-id", "data=value"]);
      expect(updateCmd).toHaveBeenCalledWith(
        ["some-id", "data=value"],
        expect.objectContaining({ strategy: "update" }),
      );

      await datum(["merge", "some-id", "data=value"]);
      expect(updateCmd).toHaveBeenCalledWith(
        ["some-id", "data=value"],
        expect.objectContaining({ strategy: "merge" }),
      );

      await datum(["rekey", "old-id", "new-id"]);
      expect(updateCmd).toHaveBeenCalledWith(
        ["old-id", "new-id"],
        expect.objectContaining({ strategy: "rekey" }),
      );
    });
  });
});
