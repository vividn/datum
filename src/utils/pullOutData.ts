import { DatumData, DatumMetadata, EitherDocument, isDatumDocument } from "../documentControl/DatumDocument";

export function pullOutData(doc: EitherDocument): { data: DatumData, meta?: DatumMetadata} {
  if (isDatumDocument(doc)) {
    return { data: doc.data, meta: doc.meta };
  } else {
    return { data: doc };
  }
}