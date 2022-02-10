import {Behavior, Component}  from './contract';

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
