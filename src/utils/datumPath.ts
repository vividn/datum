export function datumPath(path: string): string {
  const stateAwarePath =
    path === "state"
      ? "state.id"
      : path.startsWith(".")
      ? `state${path}`
      : path;
  return stateAwarePath;
}
