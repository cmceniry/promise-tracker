import PromiseTracker from '../src/libs/promise-tracker/promise-tracker.js'
import { allFromYAML } from '../src/libs/promise-tracker/contract.js';
import { promises as fs } from 'fs';
import { log } from 'console';
import { inspect } from 'util';

const pt = new PromiseTracker();

async function loadFile(pt, filename) {
    let addCount = 0;
    try {
        const data = await fs.readFile(filename, 'utf8');
        const allComponents = allFromYAML(data);
        for (const comp of allComponents) {
            pt.add(comp);
            addCount += 1;
        };
    } catch (e) {
        console.log(e);
        return 0;
    }
    return addCount;
}


// const component = process.argv.slice(-2)[0];
const componentName = process.argv.slice(-1)[0];

for (const file of process.argv.slice(2,-1).values()) {
    const addCount = await loadFile(pt, file);
    console.log(`${file} added ${addCount}`);
};

const wants = [...new Set(
    pt.Components.get(componentName)
        .map((c) => c.getWants())
        .flat()
        .map((b) => b.name)
)];
console.log(wants);

wants.forEach((w) => {
    console.log("--------");
    const r = pt.fullResolve(w, true);
    if (r.isSatisfied()) {
        console.log(`"${w}" GOOD`);
    } else {
        const needs = r.neededConditions();
        console.log(`"${w}" BAD. NEEDS: ` + needs.map((n) => `"${n}`).join(", "));
        console.log(inspect(r, false, null, true));
    }
});
