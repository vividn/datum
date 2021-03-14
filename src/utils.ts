export const inferType = (value: number | string) => {
  if (typeof value === "number") {
    return value;
  }
  if (/^null$/i.test(value)) {
    return null;
  }
  if (/^nan$/i.test(value)) {
    return Number.NaN;
  }
  if (/^-?inf(inity)?$/i.test(value)) {
    return value[0] === "-"
      ? Number.NEGATIVE_INFINITY
      : Number.POSITIVE_INFINITY;
  }
  try {
    const RJSON = require("relaxed-json");
    return RJSON.parse(value);
  } catch {}
  return value;
};

export const splitFirstEquals = (str: string): [string, string | undefined] => {
  const [first, ...eqSepValue] = str.split("=");
  if (eqSepValue.length === 0) {
    return [first, undefined];
  }
  return [first, eqSepValue.join("=")];
};

const createOrAppend = (existing: any, toAppend: any): any => {
  if (existing === undefined) {
    return toAppend;
  }
  const existingArr = Array.isArray(existing ?? [])
    ? existing ?? []
    : [existing];
  existingArr.push(toAppend);
  return existingArr;
};

const editInTerminal = async (mapFn: string): Promise<string | undefined> => {
  const { file: tmpFile } = require("tmp-promise");
  const fs = require("fs").promises;
  const child_process = require("child_process");
  const editor = process.env.EDITOR || "vi";

  const { path, cleanup } = await tmpFile();
  await fs.writeFile(path, mapFn);

  return new Promise((resolve, reject) => {
    const child = child_process.spawn(editor, [path], {
      stdio: "inherit",
    });
    child.on("exit", async (code: number) => {
      if (code !== 0) {
        resolve(undefined);
      } else {
        const newMapFn = await fs.readFile(path, "utf8");
        resolve(newMapFn);
      }
      cleanup()
    });
  });
};

module.exports = { inferType, splitFirstEquals, createOrAppend, editInTerminal };
