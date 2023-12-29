import { Card } from 'react-bootstrap';
import Mermaid from './Mermaid';
import ptdiagram from '../libs/promise-tracker/diagram';

function diagram({ pt, selectedComponent, selectedBehavior }) {
  if (!pt) {
    return "sequenceDiagram\npromiseviewer->>you: bug\nyou->>promiseviewer: submit bug report";
  };
  if (pt.is_empty()) {
    return "sequenceDiagram\nyou->>contract: add components to this simulation";
  };
  if (selectedComponent === undefined || selectedComponent === "---") {
    return "sequenceDiagram\nyou->>component: select component";
  };
  if (!pt.has_agent(selectedComponent)) {
    return "sequenceDiagram\nyou->>component: select component in this simulation";
  };
  if (pt.get_agent_wants(selectedComponent).length === 0) {
    return "sequenceDiagram\nyou->>component: select component with wants";
  };
  if (selectedBehavior === undefined || selectedBehavior === "---") {
    // Can have this try to map out everything for this component/agent
    // but not sure that that is necessary, so just show an explicit want
    return "sequenceDiagram\nyou->>component: select behavior";
  };
  if (!pt.has_behavior(selectedBehavior)) {
    return "sequenceDiagram\nyou->>behavior: select behavior";
  }
  const r = pt.resolve(selectedBehavior);
  const d = ptdiagram({
    component: selectedComponent,
    behavior: r.behavior_name,
    satisfied: r.satisfying_offers,
    unsatisfied: r.unsatisfying_offers,
  });
  return d;
}

export default function ContractGraph({ simId, pt, selectedComponent, selectedBehavior }) {
  return <Card body>
    <Mermaid id={simId} chart={diagram({ pt, selectedComponent, selectedBehavior })}></Mermaid>
  </Card>
}