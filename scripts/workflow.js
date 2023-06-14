import PromiseTracker from '../src/libs/promise-tracker/promise-tracker.js'
import { allFromYAML } from '../src/libs/promise-tracker/contract.js';
import ptworkstream from '../src/libs/promise-tracker/workstream.js';
import { promises as fs } from 'fs';

const pt = new PromiseTracker();

async function loadFile(pt, filename) {
    let addCount = 0;
    try {
        const data = await fs.readFile(filename, 'utf8');
        const allComponents = allFromYAML(data);
        for (const comp of allComponents) {
            pt.addComponent(comp);
            addCount += 1;
        };
    } catch (e) {
        console.log(e);
        return 0;
    }
    return addCount;
}

// const component = process.argv.slice(-2)[0];
const behavior  = process.argv.slice(-1)[0];

for (const file of process.argv.slice(2,-1).values()) {
    const addCount = await loadFile(pt, file);
    console.log(`${file} added ${addCount}`);
};

const r = pt.resolve(behavior);
if (r.satisfied) {
    console.log(ptworkstream(r));
} else {
    console.log(pt.getBehaviorNames());
}
