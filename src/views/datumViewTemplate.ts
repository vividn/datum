export const datumViewTemplate = String.raw`
type DocType = DatumDoc<{}>
type MapKey = unknown;
type MapValue = unknown;
type ReduceValue = unknown;

function emit(_key: MapKey, _value: MapValue): void {}

export const newDatumView: DatumView<DocType, MapKey, MapValue, ReduceValue> = {
  name: "view_name",
  emit,
  map: (doc) => {
    const data = doc.data;
    emit(data.field, null)
  },
};
`;
