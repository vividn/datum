import * as d3 from "d3";
import { MapRow } from "../views/DatumView";
import { stateChangeView } from "../views/datumViews/stateChangeView";

export async function dayview(_rows: MapRow<typeof stateChangeView>[]) {
  const width = 960;
  const height = 500;
  const margin = { top: 20, right: 20, bottom: 30, left: 40 };
  const svg = d3.create("svg").attr("viewBox", [0, 0, width, height]);

  // const data = rows;
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

  // const data = []
  // const minTime = d3.min(data, (d) => d.key[1]);
  // const maxTime = d3.max(data, (d) => d.key[1]);

  const rectangles = [
    { x: 10, y: 10, width: 200, height: 100, color: "red" },
    { x: 50, y: 50, width: 200, height: 100, color: "blue" },
    { x: 100, y: 20, width: 200, height: 100, color: "green" },
    { x: 150, y: 70, width: 200, height: 100, color: "purple" },
  ];

  // Add rectangles to the SVG
  svg
    .selectAll("rect")
    .data(rectangles)
    .enter()
    .append("rect")
    .attr("x", (d) => d.x)
    .attr("y", (d) => d.y)
    .attr("width", (d) => d.width)
    .attr("height", (d) => d.height)
    .attr("fill", (d) => d.color);

  return svg.node()?.outerHTML;
}
