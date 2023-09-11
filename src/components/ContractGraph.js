import { Card } from 'react-bootstrap';
import Mermaid from './Mermaid';
import ptdiagram from '../libs/promise-tracker/diagram';

function diagram({pt, selectedComponent, selectedBehavior}) {
  if (!pt) {
    return "sequenceDiagram\npromiseviewer->>you: bug\nyou->>promiseviewer: submit bug report";
  };
  if (pt.getComponentNames().length === 0) { // TODO: ptrs - tracker.is_empty()
    return "sequenceDiagram\nyou->>contract: add components to this simulation";
  };
  if (selectedComponent === undefined || selectedComponent === "---") {
    return "sequenceDiagram\nyou->>component: select component";
  };
  if (!pt.getComponentNames().includes(selectedComponent)) { // TODO: ptrs - tracker.has_component(component_name)
    return "sequenceDiagram\nyou->>component: select component in this simulation";
  };
  if (selectedComponent && selectedComponent !== "---" && 
      pt.Components.get(selectedComponent).map((c) => c.getWants().map((b) => b.name)).flat().length === 0) { // TODO: ptrs - tracker.has_component_wants(component_name)
    return "sequenceDiagram\nyou->>component: select component with wants";
  };
  if (selectedBehavior === undefined || selectedBehavior === "---") {
    return pt.Components.get(selectedComponent).map((c) => // TODO: ptrs - next line
      c.getWants().map((b) => b.name) // TODO: ptrs - tracker.get_component_wants(component_name)
    ).flat().map((b) => 
      ptdiagram({...pt.resolve(b), component: selectedComponent}) // TODO: ptrs - tracker.resolve(behavior_name, root_component), ptdiagram?
    ).join("\n").replaceAll(/\nsequenceDiagram/g, "\n");
  };
  if (selectedBehavior && !pt.getBehaviorNames().includes(selectedBehavior)) { // TODO: ptrs - tracker.has_behavior(behavior_name)
    return "sequenceDiagram\nyou->>behavior: select behavior in this simulation";
  };
  return ptdiagram({...pt.resolve(selectedBehavior), component: selectedComponent}); // TODO: ptrs - tracker.resolve(behavior_name, root_component), ptdiagram?
}

export default function ContractGraph({simId, pt, selectedComponent, selectedBehavior}) {
  return <Card body>
    <Mermaid id={simId} chart={diagram({pt,selectedComponent,selectedBehavior})}></Mermaid>
  </Card>
}