import { readFileSync } from "fs";
import { parse } from "smol-toml";
import { defaultConfigDir } from "../config/defaultConfigYml";

type FieldSpec = {
  kind?: "occur" | "start" | "switch";
  color?: string;
  y?: number; // 0 to 1
  height?: number; // 0 to 1
  zIndex?: number; // defaults to y
  states?: Record<string, { color?: string }>;
};

// The specs are a datum v3 kit (`[[field]]` entries), so one file installs in v3 with `datum kit install` and
// draws the same here. v3 names v2's kind, y and height verb, pos and size; color, zIndex and states match.
type KitField = {
  name: string;
  verb?: FieldSpec["kind"];
  color?: string;
  pos?: number;
  size?: number;
  zIndex?: number;
  states?: FieldSpec["states"];
};

export const specsPath =
  process.env["DATUM_SPECS"] ?? `${defaultConfigDir}/datum/my_specs.toml`;

let specs: Record<string, FieldSpec> | undefined;

function loadSpecs(file: string): Record<string, FieldSpec> {
  let text: string;
  try {
    text = readFileSync(file, "utf8");
  } catch {
    return {}; // no specs file: every field takes the md5 defaults
  }
  let fields: KitField[];
  try {
    fields = (parse(text)["field"] ?? []) as KitField[];
  } catch (e) {
    throw new Error(`${file}: ${(e as Error).message}`);
  }
  return Object.fromEntries(
    fields.map(({ name, verb, color, pos, size, zIndex, states }) => [
      name,
      { kind: verb, color, y: pos, height: size, zIndex, states },
    ]),
  );
}

export function getFieldSpec(field?: string): FieldSpec {
  specs ??= loadSpecs(specsPath);
  return specs[field ?? ""] ?? {};
}
