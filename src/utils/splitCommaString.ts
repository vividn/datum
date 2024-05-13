export function splitCommaString(str: string): string | string[] {
  // function that splits a string based on non escaped commas.
  // If the string contains no non escaped commas, it returns the string as is
  // a leading or trailing comma will return an array but not add an extra element at the beginning or end
  const splitString = str
    .replace(/(?<!\\),/g, "\xff\x00")
    .replace(/\\,/g, ",")
    .split("\xff\x00");
  if (splitString.length === 1) {
    return splitString[0];
  }
  if (splitString[0] === "") {
    splitString.shift();
  }
  if (splitString[splitString.length - 1] === "") {
    splitString.pop();
  }
  return splitString;
}
