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
        conditions: {
            type: "array",
            items: {$ref: "/promise-tracker/behavior-name.json"},
        },
    },
    required: ["name"],
    additionalProperties: false,
};

const componentSchema = {
    $id: "/promise-tracker/component.json",
    $schema: "http://json-schema.org/schema#",
    type: "object",
    properties: {
        kind: {enum: ["Component"]},
        name: {type: "string", pattern: "^[A-Za-z0-9-]+"},
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

const ajv = new Ajv({schemas: [behaviorName, behaviorSchema, componentSchema]});

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
}

export function compareBehavior(E1, E2) {
    if (E1.name < E2.name) return -1;
    if (E1.name > E2.name) return 1;
    return 0;
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
        if (!behaviorName) {
            return [...this.provides].sort(compareBehavior);
        }
        return (this.provides.filter((p) => p.name === behaviorName)).sort(compareBehavior);
    }

    getWants(behaviorName) {
        if (!behaviorName) {
            return [...this.wants].sort(compareBehavior);
        }
        return (this.wants.filter((w) => w.name === behaviorName)).sort(compareBehavior);
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
    const validate = ajv.getSchema("/promise-tracker/component.json");
    const valid = validate(d)
    if (!valid) {
        throw new Error('Schema Syntax Error', {cause: valid});
    }
    if (d["kind"] === "Component") {
        return Component.from_object(d);
    }
}

export function allFromYAML(rawdata) {
    const allDocs = yaml.loadAll(rawdata);
    const validate = ajv.getSchema("/promise-tracker/component.json");
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
        if (d["kind"] === "Component") {
            c.push(Component.from_object(d));
            return true;
        }
        error = new Error(`Unknown kind ${d["kind"]}`, {id: idx});
        return false;
    });
    if (error) {
        throw error;
    }
    return c;
}
