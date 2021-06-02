const editInTerminal = async (initialText: string): Promise<string | undefined> => {
  const { file: tmpFile } = require("tmp-promise");
  const fs = require("fs").promises;
  const child_process = require("child_process");
  const editor = process.env.EDITOR || "vi";

  const { path, cleanup } = await tmpFile();
  await fs.writeFile(path, initialText);

  return new Promise((resolve, reject) => {
    const child = child_process.spawn(editor, [path], {
      stdio: "inherit",
    });
    child.on("exit", async (code: number) => {
      if (code !== 0) {
        resolve(undefined);
      } else {
        resolve(await fs.readFile(path, "utf8"));
      }
      cleanup();
    });
  });
};

export default editInTerminal;