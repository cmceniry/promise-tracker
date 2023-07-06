import YAMLException from 'js-yaml/lib/exception';
import {Behavior, Component, compareBehavior, from_yaml, allFromYAML, Collective}  from './contract.js';

describe('names', () => {
    it('Behavior returns main and condition names', () => {
        const b = new Behavior("zb", ["ca", "cz"]);
        expect(b.getBehaviorNames()).toEqual(["ca", "cz", "zb"]);
    })

    it('Component returns all behavior and condition names', () => {
        const c = new Component(
            "a",
            [new Behavior("wa")],
            [new Behavior("pa", ["ca"])]
        );
        expect(c.getBehaviorNames()).toEqual(["ca", "pa", "wa"]);
    })
})

describe('behavior utilities', () => {
    it('compareBehavior sorts correctly', () => {
        expect([new Behavior("bb"), new Behavior("aa"), new Behavior("ab")].sort(compareBehavior).map((b) => b.name))
            .toEqual(["aa", "ab", "bb"]);
    });
});

describe('object utilities', () => {
    const a = new Component("a", [], []);
    const b = new Component("b", [], []);
    const c1 = new Component("c", [new Behavior("1")], []);
    const c2 = new Component("c", [new Behavior("2")], []);
    const d1a = new Component("d", [new Behavior("1")], []);
    const d1b = new Component("d", [new Behavior("1")], []);

    it('isEqual works - name', () => {
        expect(a.isEqual(b)).toEqual(false);
        expect(c1.isEqual(a)).toEqual(false);
    });
    it('isEqual - wants', () => {
        expect(c1.isEqual(c2)).toEqual(false);
    });
    it('isEqual - provides', () => {
        expect(d1a.isEqual(d1b)).toEqual(true);
    });

});

describe('object gets', () => {
    const a = new Component("a", [new Behavior("aw1")], [new Behavior("ap1")]);
    const b = new Component("b", [new Behavior("bw2"), new Behavior("bw1")], [new Behavior("bp2"), new Behavior("bp1")]);
    const c = new Component("c", [], [new Behavior("cp1", ["cpc1"])]);
    const d = new Component("d", [], [new Behavior("dp1", ["dp1c1"]), new Behavior("dp1", ["dp1c2"])]);

    it('getProvides 1-no-cond', () => {
        expect(a.getProvides()).toEqual([new Behavior("ap1")]);
        expect(a.getProvides("ap1")).toEqual([new Behavior("ap1")]);
    });
    it('getProvides 2-no-cond', () => {
        expect(b.getProvides()).toEqual([new Behavior("bp1"), new Behavior("bp2")]);
        expect(b.getProvides("bp2")).toEqual([new Behavior("bp2")]);
        expect(b.getProvides("bp1")).toEqual([new Behavior("bp1")]);
    });
    it('getProvides 1-cond', () => {
        expect(c.getProvides()).toEqual([new Behavior("cp1", ["cpc1"])]);
        expect(c.getProvides("cp1")).toEqual([new Behavior("cp1", ["cpc1"])]);
    });
    it('getProvides dup-with-cond', () => {
        expect(d.getProvides()).toEqual([new Behavior("dp1", ["dp1c1"]), new Behavior("dp1", ["dp1c2"])]);
        expect(d.getProvides("dp1")).toEqual([new Behavior("dp1", ["dp1c1"]), new Behavior("dp1", ["dp1c2"])]);
    });

    it('getWants 1', () => {
        expect(a.getWants().map((w) => w.name)).toEqual(["aw1"])
        expect(a.getWants("aw1").map((w) => w.name)).toEqual(["aw1"])
    });
    it('getWants 2', () => {
        expect(b.getWants().map((w) => w.name)).toEqual(["bw1", "bw2"])
        expect(b.getWants("bw1").map((w) => w.name)).toEqual(["bw1"])
    });
    it('getWants empty', () => {
        expect(a.getWants("foo")).toEqual([]);
        expect(b.getWants("foo")).toEqual([]);
    });

});

describe('component parsing', () => {
    it('basic parse', () => {
        const input = `
name: a
wants:
  - name: b
provides:
  - name: d
    conditions:
      - c
`;
        const c = from_yaml(input);
        expect(c.name).toEqual("a");
        expect(c.wants[0].name).toEqual("b");
        expect(c.provides[0].name).toEqual("d");
        expect(c.provides[0].conditions[0]).toEqual("c");
    });

    it('handles invalid yaml', () => {
        // expect(from_yaml(`name: foo\n  bar: baz`)).toEqual("foo");
        expect(() => {from_yaml(`name: foo\n  bar: baz`)}).toThrow(YAMLException);
    });

    it('handles invalid schema', () => {
        const input = `
name: foo
foo: blah
`;
        expect(() => {from_yaml(input)}).toThrow(/^Schema Syntax Error$/);
    });

    it('handles invalid name', () => {
        const input = `name: "#^)()"`;
        expect(() => {from_yaml(input)}).toThrow(/^Schema Syntax Error$/);
    });

    it('handles multiple yaml', () => {
        const input = `name: foo
---
name: bar
---
name: baz`
        expect(allFromYAML(input)).toEqual([
            new Component("foo"),
            new Component("bar"),
            new Component("baz"),
        ]);
    });

    it(`handles optional kind for Component`, () => {
        const input = `
name: a
`;
        var c = null;
        expect(() => {c = from_yaml(input)}).not.toThrow();
        expect(c instanceof Component).toBeTruthy();
        expect(c.name).toEqual("a");
    });
});

// Test to verify that collectives are parsed correctly
describe('collective parsing', () => {
    it('basic parse', () => {
        const input = `
name: a
kind: Collective
componentNames:
  - b
  - c
`;
        const c = from_yaml(input);
        expect(c.name).toEqual("a");
        expect(c.componentNames).toEqual(["b", "c"]);
    });
});
