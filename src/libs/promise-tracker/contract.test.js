import YAMLException from 'js-yaml/lib/exception';
import {Behavior, Component, from_yaml}  from './contract';

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
        expect(() => {from_yaml(input)}).toThrow(/^Syntax Error$/);
    });

    it('handles invalid name', () => {
        const input = `name: "#^)()"`;
        expect(() => {from_yaml(input)}).toThrow(/^Syntax Error$/);
    });
});
