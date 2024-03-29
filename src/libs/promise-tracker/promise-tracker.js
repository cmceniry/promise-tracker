import { compareBehavior, Collective, Component } from "./contract.js";
import { Resolution } from "./resolution.js";

export default class PromiseTracker {
    constructor() {
        this.Collectives = new Map();
        this.Components = new Map();
        this.rawComponents = new Map();
    }

    add(c) {
        switch (c.constructor) {
            case Component:
                this.addComponent(c);
                break;
            case Collective:
                this.addCollective(c);
                break;
            default:
        }
    }

    addWorkingComponent(c) {
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
    removeWorkingComponent(cn) {
        this.Components.delete(cn);
    }

    addRawComponent(c) {
        const cs = this.rawComponents.get(c.name);
        if (!cs) {
            this.rawComponents.set(c.name, [c]);
            return;
        }
        if (cs.some(ec => ec.isEqual(c))) {
            return;
        }
        this.rawComponents.set(c.name, [...cs, c]);
    }

    reComponentizeCollective(c) {
        this.removeWorkingComponent(c.name);
        if (!(c.instances) || c.instances.length === 0) {
            c.getComponentNames().forEach((cn) => {
                this.removeWorkingComponent(cn);
                const rcs = this.rawComponents.get(cn);
                if (!rcs) {
                    return;
                }
                rcs.forEach((rc) => {
                    this.addWorkingComponent(new Component(
                        c.name,
                        rc.getWants(),
                        rc.getProvides(),
                    ));
                });
            });
        }
        c.instances.forEach((i) => {
            const ic = new Component(i.name);
            this.removeWorkingComponent(i.name);
            i.components.forEach((cn) => {
                const rcs = this.rawComponents.get(cn);
                if (!rcs) {
                    return;
                }
                rcs.forEach((rc) => {
                    ic.addWants(rc.getWants());
                    ic.addProvides(rc.getProvides());
                });
                this.removeWorkingComponent(cn);
            });
            ic.reduce();
            ic.instancize(i.providesTag, i.conditionsTag);
            this.addWorkingComponent(ic);
        });
    }

    addCollective(c) {
        this.Collectives.set(c.name, c);
        this.reComponentizeCollective(c);
    }

    addComponent(c) {
        this.addRawComponent(c);
        const coll = this.getCollectiveByComponentName(c.name);
        if (coll) {
            this.reComponentizeCollective(coll);
        } else {
            this.addWorkingComponent(c);
        }
    }

    getCollectiveNames() {
        return [...this.Collectives.keys()].sort();
    }

    getCollectiveComponents(collectiveName) {
        return [...this.Collectives.values()].map((c) => c.getComponentNames()).flat().sort();
    }

    getCollectiveByComponentName(componentName) {
        for (const c of this.Collectives.values()) {
            if (c.getComponentNames().includes(componentName)) {
                return c;
            }
        }
    }

    getComponentNames() {
        return [...this.Components.keys()].sort();
    }

    getComponentVariants(name) {
        return [...this.Components.get(name)];
    }

    getBehaviorNames() {
        const cs = [...this.Components.values()];
        return [...new Set(cs.flatMap(cArray =>
            cArray.flatMap(c => 
                c.getBehaviorNames()
            )
        ))].sort();
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

    fullResolve(behaviorName, recursive = false) {
        const ret = new Resolution(behaviorName);
        this.getBehaviorProviders(behaviorName).forEach((provider) => {
            if (provider.behavior.conditions && provider.behavior.conditions.length === 0) {
                ret.addSatisfied(provider.componentName);
                return recursive ? ret : ret.toObject();
            };
            const conditionResolutions = provider.behavior.conditions.map((cd) => this.fullResolve(cd, true));
            if (conditionResolutions.some((r) => !r.isSatisfied())) {
                ret.addUnsatisfied(provider.componentName, conditionResolutions);
            } else {
                ret.addSatisfied(provider.componentName, conditionResolutions);
            }
        });
        return recursive ? ret : ret.toObject();
    }

    pruneResolve(f) {
        return Resolution.fromObject(f).prune().toObject();
    }

    resolve(behaviorName) {
        return this.pruneResolve(this.fullResolve(behaviorName));
    }
    
}
