import * as d3 from "d3";
import { MapRow } from "../views/DatumView";
import { stateChangeView } from "../views/datumViews/stateChangeView";
import { JSDOM } from "jsdom";

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

  const data = rows.map((row) => ({
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

  // const data = []
  // const minTime = d3.min(data, (d) => d.key[1]);
  // const maxTime = d3.max(data, (d) => d.key[1]);

  // const rectangles = [
  //   { x: 10, y: 10, width: 200, height: 100, color: "red" },
  //   { x: 50, y: 50, width: 200, height: 100, color: "blue" },
  //   { x: 100, y: 20, width: 200, height: 100, color: "green" },
  //   { x: 150, y: 70, width: 200, height: 100, color: "purple" },
  // ];
  //
  // d3.pairs(rows),
  //   (a, b) => {
  //     dataArea
  //       .append("rect")
  //       .attr("x", a.key[1])
  //       .attr("y", 10)
  //       .attr("width", 10)
  //       .attr("height", 10)
  //       .attr("fill", "red");
  //   };

  return svg.node()?.outerHTML;
}
