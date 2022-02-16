import { compareBehavior } from "./contract";

export default class PromiseTracker {
    constructor() {
        this.Components = new Map();
    }

    addComponent(c) {
        const cs = this.Components.get(c.name);
        if (!cs) {
            this.Components.set(c.name, [c]);
            return;
        }
        if (cs.some(ec => ec.isEqual(c))) {
            return;
        }
        this.Components.set(c.name, [...cs, c]);
    }

    getComponentNames() {
        return [...this.Components.keys()].sort();
    }

    getComponentVariants(name) {
        return [...this.Components.get(name)];
    }

    getBehaviorNames() {
        const cs = [...this.Components.values()];
        return cs.flatMap(cArray => 
            cArray.flatMap(c => 
                c.getBehaviorNames()
            )
        ).sort();
    }

    getBehaviorProviders(behaviorName) {
        let r = [];
        this.Components.forEach((v,k) => {
            const b = v
                .flatMap((c) => c.getProvides(behaviorName))
                .map((i) => ({componentName: k, behavior: i}));
            r = [...r, ...b];
        });
        return r.sort((e1,e2) => e1.componentName > e2.componentName ? 1 :
            e1.componentName < e2.componentName ? -1 : compareBehavior(e1.behavior, e2.behavior)
        )
    }

    
}
