import _ from 'lodash';
import yaml from 'js-yaml';
import Ajv from 'ajv';

const behaviorName = {
    $id: "/promise-tracker/behavior-name.json",
    $schema: "http://json-schema.org/schema#",
    type: "string",
    pattern: "^[A-Za-z0-9-]+",
};

const behaviorSchema = {
    $id: "/promise-tracker/behavior.json",
    $schema: "http://json-schema.org/schema#",
    type: "object",
    properties: {
        name: {$ref: "/promise-tracker/behavior-name.json"},
        comment: {type: "string"},
        conditions: {
            type: "array",
            items: {$ref: "/promise-tracker/behavior-name.json"},
        },
    },
    required: ["name"],
    additionalProperties: false,
};

const collectiveSchema = {
    $id: "/promise-tracker/collective.json",
    $schema: "http://json-schema.org/schema#",
    type: "object",
    properties: {
        kind: {enum: ["Collective"]},
        name: {type: "string", pattern: "^[A-Za-z0-9-]+"},
        comment: {type: "string"},
        componentNames: {
            type: "array",
            items: {type: "string", pattern: "^[A-Za-z0-9-]+"},
        },
        instances: {
            type: "array",
            items: {
                type: "object",
                properties: {
                    name: {type: "string", pattern: "^[A-Za-z0-9-]+"},
                    providesTag: {type: "string", pattern: "^[A-Za-z0-9-]+"},
                    conditionsTag: {type: "string", pattern: "^[A-Za-z0-9-]+"},
                    components: {
                        type: "array",
                        items: {type: "string", pattern: "^[A-Za-z0-9-]+"},
                    },
                },
                required: ["name"],
            },
        },
    },
    required: ["name"],
    additionalProperties: false,
}

const componentSchema = {
    $id: "/promise-tracker/component.json",
    $schema: "http://json-schema.org/schema#",
    type: "object",
    properties: {
        kind: {enum: ["Component"]},
        name: {type: "string", pattern: "^[A-Za-z0-9-]+"},
        comment: {type: "string"},
        wants: {
            type: "array",
            items: {$ref: "/promise-tracker/behavior.json"},
        },
        provides: {
            type: "array",
            items: {$ref: "/promise-tracker/behavior.json"},
        },
    },
    required: ["name"],
    additionalProperties: false,
};

const contractSchema = {
    $id: "/promise-tracker/contract.json",
    $schema: "http://json-schema.org/schema#",
    type: "object",
    discriminator: {propertyName: "kind"},
    required: ["kind", "name"],
    oneOf: [
        {$ref: "/promise-tracker/collective.json"},
        {$ref: "/promise-tracker/component.json"},
    ]
}

const ajv = new Ajv({
    schemas: [behaviorName, behaviorSchema, collectiveSchema, componentSchema, contractSchema],
    discriminator: true,
});

export class Behavior {
    constructor(name, conditions) {
        this.name = name;
        this.conditions = [];
        if (conditions) {
            this.conditions = [...conditions];
        }
    }

    getName() {
        return this.name;
    }

    getBehaviorNames() {
        return [...(new Set([this.name, ...this.conditions]))].sort();
    }

    instancize(providesTag, conditionsTag) {
        if (providesTag) {
            this.name = `${this.name} | ${providesTag}`;
        };
        if (conditionsTag) {
            if (this.conditions) {
                this.conditions = this.conditions.map((c) => `${c} | ${conditionsTag}`);
            };
        };
    }
}

export function compareBehavior(E1, E2) {
    if (E1.name < E2.name) return -1;
    if (E1.name > E2.name) return 1;
    return 0;
}

export class Collective {
    constructor(name, componentNames, instances) {
        this.name = name;
        this.componentNames = [];
        this.instances = [];
        if (componentNames) {
            this.componentNames = [...componentNames];
        };
        if (instances) {
            this.instances = [...instances];
        }
    }

    getName() {
        return this.name;
    }

    getComponentNames() {
        const ret = [...this.componentNames];
        this.instances.forEach((i) => {
            if (i.components) {
                i.components.forEach((c) => ret.push(c));
            };
        });
        return [...new Set(ret)].sort();
    }

    getInstanceNames() {
        return this.instances.map((i) => `${i.name}`).sort();
    }
}

export class Component {
    constructor(name, wants, provides) {
        this.name = name;
        this.wants = [];
        if (wants) {
            this.wants = [...wants];
        }
        this.provides = [];
        if (provides) {
            this.provides = [...provides];
        }
    }

    static from_object(obj) {
        const w = obj["wants"]?.map((b) => new Behavior(b.name));
        const p = obj["provides"]?.map((b) => new Behavior(b.name, b.conditions));
        return new Component(obj.name, w, p);
    }

    getBehaviorNames() {
        const ws = this.wants.flatMap((w) => w.getBehaviorNames());
        const ps = this.provides.flatMap((p) => p.getBehaviorNames());
        return [...new Set([...ws, ...ps])].sort();
    }

    isEqual(other) {
        return _.isEqual(this, other);
    }

    getProvides(behaviorName) {
        const f = behaviorName ? (p) => p.name === behaviorName : (p) => true;
        return this.provides.filter(f).map((p) => new Behavior(p.name, p.conditions)).sort(compareBehavior);
    }

    getWants(behaviorName) {
        const f = behaviorName ? (w) => w.name === behaviorName : (w) => true;
        return this.wants.filter(f).map((w) => new Behavior(w.name, w.conditions)).sort(compareBehavior);
    }

    addWants(wants) {
        this.wants = [...this.wants, ...wants];
    }

    addProvides(provides) {
        this.provides = [...this.provides, ...provides];
    }

    reduce() {
        const reducedProvides = new Map();
        const todoProvides = this.provides.map((p) => new Behavior(p.name, p.conditions));
        const behaviors = [...new Set(this.provides.map((p) => p.getName()))];
        var i = 0;
        while (todoProvides.length > 0) {
            // circular refence escape
            if (i > 10) { return };
            i++;
            const cur = todoProvides.shift();
            // if every condition points outside of this component, consider
            // this provider to be reduced as much as it can be
            if (cur.conditions.every((c) => !behaviors.includes(c))) {
                const rp = reducedProvides.get(cur.name);
                if (!rp) {
                    reducedProvides.set(cur.name, [cur]);
                } else {
                    reducedProvides.set(cur.name, [...rp, cur]);
                }
                continue;
            }
            // if every condition points outside or points to a reduced
            // provider, replace this condition with the dependent providers'
            // conditions
            // and do that for every dependent provider
            if (cur.conditions.every((c) => !behaviors.includes(c) || reducedProvides.has(c))) {
                const reducedConditionGroups = cur.conditions.map((c) => {
                    if (!behaviors.includes(c)) {
                        return [c];
                    }
                    return reducedProvides.get(c).map((b) => b.conditions);
                })
                    .reduce((a,c) => a.flatMap((x) => c.map((y) => [x,y].flat())))
                    .map((conditions) => new Behavior(cur.name, conditions));
                const rp = reducedProvides.get(cur.name);
                if (!rp) {
                    reducedProvides.set(cur.name, reducedConditionGroups);
                } else {
                    reducedProvides.set(cur.name, [...rp, ...reducedConditionGroups]);
                }
                continue;
            };
            // Put it back at the end of the list to try to reduce
            todoProvides.push(cur);
        }
        this.provides = [...reducedProvides.values()].flat().sort(compareBehavior);
        // TODO wants
    }

    instancize(providesTag, conditionsTag) {
        this.provides.forEach((p) => p.instancize(providesTag, conditionsTag));
    }
}

export class SchemaSyntaxError extends Error {
    constructor(message, {cause, errors, idx}) {
        super(message);
        this.name = "SchemaSyntaxError";
        this.cause = cause;
        this.idx = idx;
        this.errors = errors;
    }
}

export function from_yaml(rawdata) {
    const d = yaml.load(rawdata);
    if (!("kind" in d)) {
        d["kind"] = "Component";
    }
    const validate = ajv.getSchema("/promise-tracker/contract.json");
    const valid = validate(d)
    if (!valid) {
        throw new Error('Schema Syntax Error', {cause: valid});
    }
    switch (d["kind"]) {
        case "Component":
            return Component.from_object(d);
        case "Collective":
            return new Collective(d.name, d.componentNames, d.instances);
        default:
            return null;
    }
}

export function allFromYAML(rawdata) {
    const allDocs = yaml.loadAll(rawdata);
    const validate = ajv.getSchema("/promise-tracker/contract.json");
    const c = [];
    let error = null;
    allDocs.every((d, idx) => {
        if (!("kind" in d)) {
            d["kind"] = "Component";
        };
        const valid = validate(d);
        if (!valid) {
            error = new SchemaSyntaxError('Schema Syntax Error', {cause: valid, idx: idx, errors: validate.errors});
            // error = new Error(`Schema Syntax Error`, {cause: valid, id: idx});
            return false;
        }
        switch (d["kind"]) {
            case "Component":
                c.push(Component.from_object(d));
                return true;
            case "Collective":
                c.push(new Collective(d.name, d.componentNames, d.instances));
                return true;
            default:
                error = new Error(`Unknown kind ${d["kind"]}`, {id: idx});
                return false;
        }
    });
    if (error) {
        throw error;
    }
    return c;
}
