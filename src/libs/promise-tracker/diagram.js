function d({ component, behavior, satisfied, unsatisfied }) {
  let ret = []
  if (satisfied && satisfied.length > 0) {
    ret.push("rect rgb(0,255,0)");
    satisfied.forEach((e) => {
      ret.push(`    ${component} ->> ${e.agent_name}: ${behavior}`);
      if (e.resolved_conditions) {
        e.resolved_conditions.forEach((c) => {
          const child = d({
            behavior: c.behavior_name,
            component: e.agent_name,
            satisfied: c.satisfying_offers,
            unsatisfied: c.unsatisfying_offers,
          });
          ret = [...ret, ...(child.map((l) => "    " + l))];
        });
      };
    });
    ret.push("end");
  }
  if (unsatisfied && unsatisfied.length > 0) {
    ret.push("rect rgb(255,0,0)");
    unsatisfied.forEach((e) => {
      ret.push(`    ${component} ->> ${e.agent_name}: ${behavior}`);
      if (e.resolved_conditions) {
        e.resolved_conditions.forEach((c) => {
          const child = d({
            behavior: c.behavior_name,
            component: e.agent_name,
            satisfied: c.satisfying_offers,
            unsatisfied: c.unsatisfying_offers,
          });
          ret = [...ret, ...(child.map((l) => "    " + l))];
        });
      };
    });
    ret.push("end");
  }
  if (!(unsatisfied && unsatisfied.length > 0) && !(satisfied && satisfied.length > 0)) {
    ret.push("rect rgb(255,0,0)");
    ret.push(`    ${component} -X ${component}: ${behavior}`);
    ret.push("end");
  }
  return ret;
};

export default function diagram(input) {
  return ["sequenceDiagram", ...(d(input).map((l) => "    " + l))].join("\n");
};