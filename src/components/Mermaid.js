import React from "react";
import mermaid from "mermaid";

export default function Mermaid({chart}) {
  let rendering = ""
  try {
    mermaid.parse(chart);
    rendering = mermaid.render('randomdiv', chart);
  } catch (e) {
    console.log(e);
  }
  return <div dangerouslySetInnerHTML={{__html: rendering}}/>;
}
