describe("mainArgs", () => {
  it("should display the top level help message when no command is given");
  it(
    "should display the top level help message when an unknown command is given and report an error",
  );
  it("should display the top level help message when the --help flag is given");
  it("should display the top level help message with the help command");
  it(
    "should display a subcommands help message if the command is given with --help",
  );
  it("should run a command when a valid command is given");
});
