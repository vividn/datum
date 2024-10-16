import md5 from "md5";

export function md5Color(str: string) {
  return "#" + md5(str).substring(0, 6);
}
