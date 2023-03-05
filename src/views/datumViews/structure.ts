import { _emit } from "../emit";
import { EitherDocument } from "../../documentControl/DatumDocument";
import { DatumView } from "../DatumView";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

export const structuresView: DatumView<EitherDocument> = {
  name: "datum_structures",
  map: (doc) => {
    function sortedSubkeys(obj: { [key: string]: any }): string[][] {
      const subkeysByOrder: string[][] = [];
      const topOrderKeys: string[] = [];
      const keys = Object.keys(obj).sort();
      for (const key of keys) {
        topOrderKeys.push(key);
        const val = obj[key];
        if (typeof val === "object" && val !== null && !Array.isArray(val)) {
          sortedSubkeys(val).map((subkeyArray, index) => {
            const prefixedSubKeys = subkeyArray.map(
              (subkey) => key + "." + subkey
            );
            (subkeysByOrder[index] = subkeysByOrder[index] || []).push(
              ...prefixedSubKeys
            );
          });
        }
      }
      return [topOrderKeys, ...subkeysByOrder];
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
    function sortedSubkeys(obj: { [key: string]: any }): string[][] {
      const subkeysByOrder: string[][] = [];
      const topOrderKeys: string[] = [];
      const keys = Object.keys(obj).sort();
      for (const key of keys) {
        topOrderKeys.push(key);
        const val = obj[key];
        if (typeof val === "object" && val !== null && !Array.isArray(val)) {
          sortedSubkeys(val).map((subkeyArray, index) => {
            const prefixedSubKeys = subkeyArray.map(
              (subkey) => key + "." + subkey
            );
            (subkeysByOrder[index] = subkeysByOrder[index] || []).push(
              ...prefixedSubKeys
            );
          });
        }
      }
      return [topOrderKeys, ...subkeysByOrder];
    }
    if (doc.data) {
      const structure = sortedSubkeys(doc.data);
      emit(structure, null);
    }
  },
  reduce: {
    default: "_count",
    fieldList: (keysAndDocIds: [string[][], string][], values, rereduce) => {
      if (!rereduce) {
        return keysAndDocIds.reduce((accum: string[], keyAndDocId) => {
          const [listOfFields] = keyAndDocId;
          listOfFields.forEach((fieldsOfOrder) => {
            fieldsOfOrder.forEach((field) => {
              if (!accum.includes(field)) {
                accum.push(field);
              }
            });
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
