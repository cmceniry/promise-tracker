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

    resolve(behaviorName) {
        const possiblePaths = this.getBehaviorProviders(behaviorName);
        if (possiblePaths.length === 0) {
            return {
                behavior: behaviorName,
                unsatisfied: [],
            };
        }
        let satisfiedPaths = [];
        let unsatisfiedPaths = [];
        let conditionalPaths = [];
        possiblePaths.forEach((v) => {
            if (v.behavior.conditions && v.behavior.conditions.length === 0) {
                satisfiedPaths.push({component: v.componentName});
            } else {
                conditionalPaths.push(v)
            }
        });
        if (satisfiedPaths.length > 0) {
            return {
                behavior: behaviorName,
                satisfied: satisfiedPaths,
            };
        }
        conditionalPaths.forEach((cp) => {
            const child = {
                component: cp.componentName,
                conditions: [],
            }
            cp.behavior.conditions.forEach((cd) => {
                const r = this.resolve(cd);
                child.conditions.push(r);
            });
            // Must be ANDed here - if any unsatisfied, then unsatisfied
            if (child.conditions.filter((r) => r.unsatisfied).length > 0) {
                unsatisfiedPaths.push(child);
            } else {
                satisfiedPaths.push(child);
            }
        });
        // ORed here - if even one satisfied, then satisfied
        if (satisfiedPaths.length > 0) {
            return {
                behavior: behaviorName,
                satisfied: satisfiedPaths,
            }
        }
        return {
            behavior: behaviorName,
            unsatisfied: unsatisfiedPaths,
        }
    }
    
}
