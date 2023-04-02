import { _emit } from "../emit";
import { EitherDocument } from "../../documentControl/DatumDocument";
import { DatumView } from "../DatumView";

type DocType = EitherDocument;
type MapKey = string[][];
type MapValue = null;
type ReduceValue = number;

function emit(_key: MapKey, _value: MapValue): void {
  _emit(_key, _value);
}

export const structuresView: DatumView<DocType, MapKey, MapValue, ReduceValue> =
  {
    name: "datum_structures",
    emit,
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
