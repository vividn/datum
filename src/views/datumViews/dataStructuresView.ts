import { DatumView, ReduceFunction } from "../DatumView";
import { EitherDocument } from "../../documentControl/DatumDocument";
import { _emit } from "../emit";

type DocType = EitherDocument;
type MapKey = string[][];
type MapValue = null;
type ReduceValue = number;
type NamedReduceValues = {
  fieldList: string[];
};

function emit(_key: MapKey, _value: MapValue): void {
  _emit(_key, _value);
}

const fieldListReduce: ReduceFunction<
  MapKey,
  MapValue,
  NamedReduceValues["fieldList"]
> = (keysAndDocIds, values, rereduce) => {
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
    return values.reduce((accum, listOfFields) => {
      listOfFields.forEach((field) => {
        if (!accum.includes(field)) {
          accum.push(field);
        }
      });
      return accum;
    });
  }
};
export const dataStructuresView: DatumView<
  DocType,
  MapKey,
  MapValue,
  ReduceValue,
  NamedReduceValues
> = {
  name: "datum_data_structures",
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
              (subkey) => key + "." + subkey,
            );
            (subkeysByOrder[index] = subkeysByOrder[index] || []).push(
              ...prefixedSubKeys,
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
  reduce: "_count",
  namedReduce: {
    fieldList: fieldListReduce,
  },
};
