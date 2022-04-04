import { DatumView } from "./viewDocument";
import * as datumViews from "./datumViews";

const typedDatumViews = datumViews as { [name: string]: DatumView };

export function getAllDatumViews(): DatumView[] {
  const allDatumViews: DatumView[] = [];
  for (const viewName in typedDatumViews) {
    allDatumViews.push(typedDatumViews[viewName]);
  }
  return allDatumViews;
}

export async function getDbDatumViews(dbName: string): Promise<DatumView[]> {
  const dbViews = (await import(`../../projects/${dbName}`)) as {
    [name: string]: DatumView;
  };
  const allDbViews: DatumView[] = [];
  for (const viewName in dbViews) {
    allDbViews.push(dbViews[viewName]);
  }
  return allDbViews;
}
