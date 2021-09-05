export function splitRawAndFields(str: string): string[] {
  // split apart and also replace the escaped %s with normal percents
  return str
    .replace(/(?<!\\)%/g, "\xff\x00")
    .replace(/\\%/g, "%")
    .split("\xff\x00");
}
