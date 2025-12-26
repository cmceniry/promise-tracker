import React from 'react';
import { Card } from 'react-bootstrap';
import './ContractText.css';

// returns a list of <li> elements showing the options for the component+behavior
function prettyPrintContract({ component, behavior, satisfied, unsatisfied }) {
  if ((satisfied === undefined || satisfied.length === 0) &&
      (unsatisfied === undefined || unsatisfied.length === 0)) {
    return <li className="contract-text-possible">{`${component} --> ${behavior} UNSATISFIED`}</li>;
  }

  let options = [];
  if (satisfied !== undefined && satisfied.length > 0) {
    satisfied.forEach((e, idx) => {
      if (e.resolved_conditions && e.resolved_conditions.length > 0) {
        let children = <ul className="contract-text-list">
          {e.resolved_conditions.map((c, cIdx) => {
            const elem = prettyPrintContract({
                component: e.agent_name,
                behavior: c.behavior_name,
                satisfied: c.satisfying_offers,
                unsatisfied: c.unsatisfying_offers,
            });
            return React.cloneElement(elem, { key: cIdx });
          })} </ul>;
        options.push(<li key="satisfied-${idx}" className="contract-text-option">{`OPTION: ${e.agent_name}`}{children}</li>);
      } else {
        options.push(<li key="satisfied-${idx}" className="contract-text-option">{`OPTION: ${e.agent_name}`}</li>);
      }
    });
  }
  if (unsatisfied !== undefined && unsatisfied.length > 0) {
    unsatisfied.forEach((e, idx) => {
      if (e === undefined || e.resolved_conditions === undefined || e.resolved_conditions.length === 0) {
        options.push(<li key="unsatisfied-${idx}" className="contract-text-error">{`ERROR: ${e.agent_name}`}</li>);
        return;
      }
      let children = <ul className="contract-text-list">
        {e.resolved_conditions.map((c, cIdx) => {
          const elem = prettyPrintContract({
              component: e.agent_name,
              behavior: c.behavior_name,
              satisfied: c.satisfying_offers,
              unsatisfied: c.unsatisfying_offers,
          });
          return React.cloneElement(elem, { key: cIdx });
      })} </ul>;
      options.push(<li key="unsatisfied-${idx}" className="contract-text-possible">{`POSSIBLE: ${e.agent_name}`}{children}</li>);
    });
  }

  const contractClass = (satisfied && satisfied.length > 0) ? "contract-text-option" : "contract-text-possible";

  return <li className={contractClass}>
    {`${component} --> ${behavior}`}
    <ul className="contract-text-list">
      {options}
    </ul> 
  </li>;
}

export default function ContractText({ pt, selectedComponent, selectedBehavior }) {
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
  const contractText = prettyPrintContract({
    component: selectedComponent,
    behavior: r.behavior_name,
    satisfied: r.satisfying_offers,
    unsatisfied: r.unsatisfying_offers,
  });

  return (
    <Card body className="contract-text-card">
      <ul className="contract-text-list">
        {contractText}
      </ul>
    </Card>
  );
}