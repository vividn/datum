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
