import { file as tmpFile } from "tmp-promise";
import { promises as fs } from "fs";
import RJSON from "relaxed-json";

import child_process from "child_process";
import { GenericObject } from "../GenericObject";
import { MyError } from "../errors";

export class EditorError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, EditorError.prototype);
  }
}

export async function editInTerminal(initialText: string): Promise<string> {
  const editor = process.env.EDITOR || "vi";

  const { path, cleanup } = await tmpFile();
  await fs.writeFile(path, initialText);

  return new Promise((resolve, reject) => {
    const child = child_process.spawn(editor, [path], {
      stdio: "inherit",
    });
    child.on("exit", async (code: number) => {
      if (code !== 0) {
        reject(
          new EditorError(`${editor} returned non-zero exit code: ${code}`),
        );
      } else {
        resolve(await fs.readFile(path, "utf8"));
      }
      await cleanup();
    });
  });
}

export async function editJSONInTerminal(
  object: GenericObject,
): Promise<GenericObject> {
  const objectStr = JSON.stringify(object, null, 4);
  const editedObjectStr = await editInTerminal(objectStr);
  return RJSON.parse(editedObjectStr);
}
