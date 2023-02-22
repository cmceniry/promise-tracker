import React from "react";
import mermaid from "mermaid";

export default function Mermaid({chart,id}) {
  const chartId = id === undefined ? "promisechart" : id
  let rendering = ""
  try {
    mermaid.initialize({
      startOnLoad: false,
      sequence: {
        useMaxWidth: false,
      },
    });
    mermaid.parse(chart);
    rendering = mermaid.render(chartId, chart);
  } catch (e) {
    console.log(e);
  }
  return <div style={{overflow: "scroll"}} dangerouslySetInnerHTML={{__html: rendering}}/>;
}
