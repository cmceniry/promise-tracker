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

    getBehaviorNames() {
        const ws = this.wants.flatMap((w) => w.getBehaviorNames());
        const ps = this.provides.flatMap((p) => p.getBehaviorNames());
        return [...new Set([...ws, ...ps])].sort();
    }

    isEqual(other) {
        return _.isEqual(this, other);
    }

}

export function from_yaml(rawdata) {
    const d = yaml.load(rawdata);

    const validate = ajv.getSchema("/promise-tracker/component.json");
    const valid = validate(d)
    if (!valid) {
        throw new Error('Syntax Error', {cause: valid});
    }

    const w = d["wants"]?.map((b) => new Behavior(b.name));
    const p = d["provides"]?.map((b) => new Behavior(b.name, b.conditions));
    return new Component(d.name, w, p)
}
