import _emit from "../emit";
import { EitherDocument } from "../../documentControl/DatumDocument";
import { DatumView } from "../viewDocument";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

export const structuresView: DatumView<EitherDocument> = {
  name: "datum_structures",
  map: (doc) => {
    function sortedSubkeys(obj: { [key: string]: any }): string[] {
      const listOfDeepKeys: string[] = [];
      const keys = Object.keys(obj).sort();
      for (const key of keys) {
        listOfDeepKeys.push(key);
        const val = obj[key];
        if (typeof val === "object" && val !== null && !Array.isArray(val)) {
          const subKeys = sortedSubkeys(val);
          const prefixedSubKeys = subKeys.map((subkey) => key + "." + subkey);
          listOfDeepKeys.push(...prefixedSubKeys);
        }
      }
      return listOfDeepKeys;
    }

    delete (doc as any)["_rev"];
    delete (doc as any)["_id"];
    const structure = sortedSubkeys(doc);

    emit(structure, null);
  },
  reduce: "_count",
};

export const dataStructuresView: DatumView<EitherDocument> = {
  name: "datum_data_structures",
  map: (doc) => {
    function sortedSubkeys(obj: { [key: string]: any }): string[] {
      const listOfDeepKeys: string[] = [];
      const keys = Object.keys(obj).sort();
      for (const key of keys) {
        listOfDeepKeys.push(key);
        const val = obj[key];
        if (typeof val === "object" && val !== null && !Array.isArray(val)) {
          const subKeys = sortedSubkeys(val);
          const prefixedSubKeys = subKeys.map((subkey) => key + "." + subkey);
          listOfDeepKeys.push(...prefixedSubKeys);
        }
      }
      return listOfDeepKeys;
    }
    if (doc.data) {
      const structure = sortedSubkeys(doc.data);
      emit(structure, null);
    }
  },
  reduce: {
    count: "_count",
    fieldList: (keysAndDocIds: [string[], string][], values, rereduce) => {
      if (!rereduce) {
        return keysAndDocIds.reduce((accum: string[], keyAndDocId) => {
          const [listOfFields] = keyAndDocId;
          listOfFields.forEach((field) => {
            if (!accum.includes(field)) {
              accum.push(field);
            }
          });
          return accum;
        }, [] as string[]);
      } else {
        return (values as string[][]).reduce((accum, listOfFields) => {
          listOfFields.forEach((field) => {
            if (!accum.includes(field)) {
              accum.push(field);
            }
          });
          return accum;
        });
      }
    },
  },
};
