function d({component, behavior, satisfied, unsatisfied}) {
  let ret = []
  if (satisfied) {
    ret.push("rect rgb(0,255,0)");
    satisfied.forEach((e) => {
      ret.push(`    ${component} ->> ${e.component}: ${behavior}`);
      if (e.conditions) {
        e.conditions.forEach((c) => {
          const child = d({
            behavior: c.behavior,
            component: e.component,
            satisfied: c.satisfied,
            unsatisfied: c.unsatisfied,
          });
          ret = [...ret, ...(child.map((l) => "    " + l))];  
        });
      };
    });
    ret.push("end");
  }
  if (unsatisfied) {
    ret.push("rect rgb(255,0,0)");
    if (unsatisfied.length > 0) {
      unsatisfied.forEach((e) => {
        ret.push(`    ${component} ->> ${e.component}: ${behavior}`);
        if (e.conditions) {
          e.conditions.forEach((c) => {
            const child = d({
              behavior: c.behavior,
              component: e.component,
              satisfied: c.satisfied,
              unsatisfied: c.unsatisfied,
            });
            ret = [...ret, ...(child.map((l) => "    " + l))];
          });
        };
      });
    } else {
      ret.push(`    ${component} -X ${component}: ${behavior}`);
    }
    ret.push("end");
  }
  return ret;
};

export default function diagram(input) {
  return ["sequenceDiagram", ...(d(input).map((l) => "    " + l))].join("\n");
};