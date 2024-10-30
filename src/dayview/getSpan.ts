import md5 from "md5";
import { DAYVIEW_SPANS } from "../field/tempExampleSpans";

export function getSpan(field: string): [number, number] {
  const customSpan = DAYVIEW_SPANS[field];
  if (customSpan) {
    return [customSpan[0], customSpan[1] - customSpan[0]];
  }

  const hash = md5(field);
  const y1 = parseInt(hash.slice(0, 8), 16) / Math.pow(2, 32);
  return [y1, 0.03];
}
