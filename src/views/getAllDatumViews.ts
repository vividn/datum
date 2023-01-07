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

export async function getDbDatumViews({
  dbName,
  projectDir,
}: {
  dbName: string;
  projectDir: string;
}): Promise<DatumView[]> {
  let dbViews: { [key: string]: DatumView };
  try {
    dbViews = (await import(`${projectDir}/${dbName}/views`)) as {
      [name: string]: DatumView;
    };
  } catch (e: any) {
    if (e.message.startsWith("Cannot find module")) {
      console.warn(`no folder for db found in ${projectDir}/${dbName}`);
      return [];
    } else {
      throw e;
    }
  }
  const allDbViews: DatumView[] = [];
  for (const viewName in dbViews) {
    allDbViews.push(dbViews[viewName]);
  }
  return allDbViews;
}
