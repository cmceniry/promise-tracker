function w({behavior, satisfied, unsatisfied}) {
  if (!satisfied) {
    throw(new Error("nope"));
  };
  let components = {};
  let steps = new Set();
  const s = satisfied[0];
  if (!components[s.component]) {
    components[s.component] = new Set();;
  };
  components[s.component].add(behavior);
  if (s.conditions) {
    s.conditions.forEach((cond) => {
      steps.add(cond.behavior + " ->> " + behavior);
      const condw = w({behavior: cond.behavior, satisfied: cond.satisfied, unsatisfied: cond.unsatisfied});
      for (const c in condw.components) {
        if (!components[c]) {
          components[c] = new Set();
        };
        components[c] = [...components[c], ...condw.components[c]];
        steps = new Set([...steps, ...condw.steps]);
      }
    });
  }
  return {components: components, steps: steps};
};

export default function workstream(input) {
  let subgraphs = [];
  let connections = [];;
  const r = w(input);
  for (const c in r.components) {
    subgraphs.push("subgraph " + c);
    r.components[c].forEach((b) => subgraphs.push("  " + b));
    subgraphs.push("end");
  };
  connections = [...r.steps];
  connections.sort();
  return [
    "flowchart TD", 
    ...(subgraphs.map((l)   => "    " + l)),
    ...(connections.map((l) => "    " + l)),
  ].join("\n");
};
