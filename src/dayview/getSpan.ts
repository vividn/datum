import md5 from "md5";
import { FIELD_SPECS } from "../field/mySpecs";

export function getSpan(field: string): [number, number] {
  const spec = FIELD_SPECS[field] ?? {};
  const { y, height } = spec;

  const hash = md5(field);
  const y1 = y ?? parseInt(hash.slice(0, 8), 16) / Math.pow(2, 32);
  const h = height ?? 0.05;
  return [y1, h];
}
