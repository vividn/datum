export default function jClone<T>(obj: T): T {
  return JSON.parse(JSON.stringify(obj));
}
