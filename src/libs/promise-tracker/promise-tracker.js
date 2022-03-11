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

    fullResolve(behaviorName) {
        const ret = {
            behavior: behaviorName,
            satisfied: [],
            unsatisfied: [],
        };
        this.getBehaviorProviders(behaviorName).forEach((v) => {
            if (v.behavior.conditions && v.behavior.conditions.length === 0) {
                ret.satisfied.push({component: v.componentName});
                return;
            };
            const child = {
                component: v.componentName,
                conditions: [],
            }
            v.behavior.conditions.forEach((cd) => {
                const r = this.resolve(cd);
                child.conditions.push(r);
            });
            // Must be ANDed here - if any unsatisfied, then unsatisfied
            if (child.conditions.filter((r) => r.unsatisfied).length > 0) {
                ret.unsatisfied.push(child);
            } else {
                ret.satisfied.push(child);
            }
        });
        return ret;
    }

    pruneResolve(f) {
        const ret = {behavior: f.behavior};
        if (f.satisfied && f.satisfied.length > 0) {
            ret.satisfied = [];
            f.satisfied.forEach((se) => {
                if (se.conditions) {
                    ret.satisfied.push({
                        component: se.component,
                        conditions: se.conditions.map((c) => this.pruneResolve(c)),
                    });
                } else {
                    ret.satisfied.push({
                        component: se.component,
                    });
                };
            });
            return ret;
        }
        ret.unsatisfied = []
        if (!f.unsatisfied || f.unsatisfied.length === 0) {
            return ret;
        }
        f.unsatisfied.forEach((ue) => {
            ret.unsatisfied.push({
                component: ue.component,
                conditions: ue.conditions.map((c) => this.pruneResolve(c)),
            });
        });
        return ret;
    }

    resolve(behaviorName) {
        return this.pruneResolve(this.fullResolve(behaviorName));
    }
    
}
