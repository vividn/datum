import {
  DatumData,
  DatumMetadata,
  EitherPayload,
  isDatumPayload,
} from "../documentControl/DatumDocument";

export function pullOutData(doc: EitherPayload): {
  data: DatumData;
  meta?: DatumMetadata;
} {
  if (isDatumPayload(doc)) {
    return { data: doc.data, meta: doc.meta };
  } else {
    return { data: doc };
  }
}
