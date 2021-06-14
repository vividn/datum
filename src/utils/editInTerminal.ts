import { file as tmpFile } from "tmp-promise";
import { promises as fs } from "fs";

import child_process from "child_process";

const editInTerminal = async (
  initialText: string
): Promise<string | undefined> => {
  const editor = process.env.EDITOR || "vi";

  const { path, cleanup } = await tmpFile();
  await fs.writeFile(path, initialText);

  return new Promise((resolve) => {
    const child = child_process.spawn(editor, [path], {
      stdio: "inherit",
    });
    child.on("exit", async (code: number) => {
      if (code !== 0) {
        resolve(undefined);
      } else {
        resolve(await fs.readFile(path, "utf8"));
      }
      await cleanup();
    });
  });
};

export default editInTerminal;
