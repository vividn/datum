import { DatumView } from "./DatumView";
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
  subfolder,
}: {
  dbName: string;
  projectDir: string;
  subfolder?: string;
}): Promise<DatumView[]> {
  let dbViews: { [key: string]: DatumView };
  subfolder ??= "views";
  try {
    dbViews = (await import(`${projectDir}/${dbName}/${subfolder}`)) as {
      [name: string]: DatumView;
    };
  } catch (e: any) {
    if (e.message.startsWith("Cannot find module")) {
      return [];
    } else {
      throw e;
    }
  }
  const allDbViews: DatumView[] = [];
  for (const viewName in dbViews) {
    allDbViews.push(dbViews[viewName]);
  }
  console.debug({allDbViews});
  return allDbViews;
}
