import { _emit } from "../emit";
import { DatumView } from "../viewDocument";
import { DatumDocument } from "../../documentControl/DatumDocument";

function emit(doc: unknown, value: unknown) {
  _emit(doc, value);
}

export const datumV1View: DatumView<DatumDocument> = {
  name: "datum_v1_view",
  map: (doc) => {
    const data = doc.data;
    if (!data.occurTime || !data.occurUtcOffset) {
      return;
    }

    const offset = data.occurUtcOffset;
    const msOffset = offset * 60 * 60 * 1000;
    const dateTime = new Date(data.occurTime);
    const offsetDateTime = new Date(dateTime.getTime() - msOffset);

    const [dateStr, timeStr] = offsetDateTime.toISOString().split(/T|Z/);

    const offsetPolarity = offset >= 0 ? "+" : "-";
    const offsetHour =
      Math.abs(offset) >= 10
        ? Math.abs(offset) + "00"
        : "0" + Math.abs(offset) + "00";
    const offsetStr = offsetPolarity + offsetHour;

    const duration = data.dur || "";
    emit(data.occurTime, [dateStr, timeStr, offsetStr, duration]);
  },
};
