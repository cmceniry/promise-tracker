import React from "react";
import mermaid from "mermaid";

export default function Mermaid({chart}) {
  let rendering = ""
  try {
    rendering = mermaid.render('foo', chart);
  } catch (e) {
    console.log(e);
  }
  return <div dangerouslySetInnerHTML={{__html: rendering}}/>;
}
