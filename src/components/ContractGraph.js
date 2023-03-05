import { Card } from 'react-bootstrap';
import Mermaid from './Mermaid';
import ptdiagram from '../libs/promise-tracker/diagram';

function diagram({pt, selectedComponent, selectedBehavior}) {
  if (!pt) {
    return "sequenceDiagram\npromiseviewer->>you: bug\nyou->>promiseviewer: submit bug report";
  };
  if (pt.getComponentNames().length === 0) {
    return "sequenceDiagram\nyou->>contract: add components to this simulation";
  };
  if (selectedComponent === undefined || selectedComponent === "---") {
    return "sequenceDiagram\nyou->>component: select component";
  };
  if (!pt.getComponentNames().includes(selectedComponent)) {
    return "sequenceDiagram\nyou->>component: select component in this simulation";
  };
  if (selectedComponent && selectedComponent !== "---" && 
      pt.Components.get(selectedComponent).map((c) => c.getWants().map((b) => b.name)).flat().length === 0) {
    return "sequenceDiagram\nyou->>component: select component with wants";
  };
  if (selectedBehavior === undefined || selectedBehavior === "---") {
    return pt.Components.get(selectedComponent).map((c) =>
      c.getWants().map((b) => b.name)
    ).flat().map((b) => 
      ptdiagram({...pt.resolve(b), component: selectedComponent})
    ).join("\n").replaceAll(/\nsequenceDiagram/g, "\n");
  };
  if (selectedBehavior && !pt.getBehaviorNames().includes(selectedBehavior)) {
    return "sequenceDiagram\nyou->>behavior: select behavior in this simulation";
  };
  return ptdiagram({...pt.resolve(selectedBehavior), component: selectedComponent});
}

export default function ContractGraph({simId, pt, selectedComponent, selectedBehavior}) {
  return <Card body>
    <Mermaid id={simId} chart={diagram({pt,selectedComponent,selectedBehavior})}></Mermaid>
  </Card>
}