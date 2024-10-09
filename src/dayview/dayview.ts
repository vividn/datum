import * as d3 from "d3";
import { MapRow } from "../views/DatumView";
import { stateChangeView } from "../views/datumViews/stateChangeView";
import { JSDOM } from "jsdom";
import { DatumState } from "../state/normalizeState";
import md5 from "md5";

function md5Color(str: string) {
  return "#" + md5(str).substring(0, 6);
}

const rows: MapRow<typeof stateChangeView>[] = [
  {
    key: ["field", "2024-09-26T03:00:00.000Z"],
    value: ["state1", "state2"],
    id: "",
  },
  {
    key: ["field", "2024-09-26T04:00:00.000Z"],
    value: ["state2", "state3"],
    id: "",
  },
  {
    key: ["field", "2024-09-26T05:00:00.000Z"],
    value: ["state3", "state1"],
    id: "",
  },
  {
    key: ["field", "2024-09-26T06:00:00.000Z"],
    value: ["state1", "state2"],
    id: "",
  },
  {
    key: ["field", "2024-09-26T10:00:00.000Z"],
    value: ["state2", "state3"],
    id: "",
  },
  {
    key: ["field", "2024-09-26T11:00:00.000Z"],
    value: ["state3", "state1"],
    id: "",
  },
  {
    key: ["field", "2024-09-26T12:00:00.000Z"],
    value: ["state1", "state2"],
    id: "",
  },
  {
    key: ["field", "2024-09-26T13:00:00.000Z"],
    value: ["state2", "state3"],
    id: "",
  },
  {
    key: ["field", "2024-09-26T14:00:00.000Z"],
    value: ["state3", "state1"],
    id: "",
  },
];

export async function dayview(_rows: MapRow<typeof stateChangeView>[]) {
  const document = new JSDOM().window.document;
  const width = 960;
  const height = 500;
  const margin = { top: 20, right: 20, bottom: 30, left: 40 };
  const dataWidth = width - margin.left - margin.right;
  const dataHeight = height - margin.top - margin.bottom;

  const svg = d3
    .select(document.body)
    .append("svg")
    .attr("viewBox", [0, 0, width, height]);

  const _background = svg
    .append("rect")
    .attr("width", width)
    .attr("height", height)
    .attr("fill", "lightgray");

  const dataArea = svg
    .append("g")
    .attr("transform", `translate(${margin.left},${margin.top})`);

  type DBlock = {
    field: string;
    time: Date;
    state: DatumState;
  };

  const data: DBlock[] = rows.map((row) => ({
    field: row.key[0],
    time: new Date(row.key[1]),
    state: row.value[1],
  }));

  const timeScale = d3
    .scaleTime()
    .domain([data.at(0)!.time, data.at(-1)!.time])
    .range([0, dataWidth]);

  const xAxis = d3.axisBottom(timeScale);

  dataArea
    .append("g")
    .attr("transform", `translate(0,${dataHeight})`)
    .call(xAxis);

  const dataPairs = d3.pairs(data);
  type Pair = [DBlock, DBlock];

  dataArea
    .selectAll(".rect")
    .data(dataPairs)
    .enter()
    .append("rect")
    .attr("class", "rect")
    .attr("x", (d: Pair) => timeScale(d[0].time))
    .attr("y", 10)
    .attr("width", (d) => timeScale(d[1].time) - timeScale(d[0].time))
    .attr("height", 10)
    .attr("fill", (d) => md5Color(String(d[0].state)));

  return svg.node()?.outerHTML;
}
