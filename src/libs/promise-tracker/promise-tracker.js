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
    
}
