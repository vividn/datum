export function getMigrationViewName(shortName: string): string {
  return `migrate__${shortName}`;
}
export function getMigrationId(shortName: string): string {
  return `_design/${getMigrationViewName(shortName)}`;
}
