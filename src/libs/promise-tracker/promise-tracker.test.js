import { Behavior, Component } from './contract';
import PromiseTracker from './promise-tracker';

describe('Adding Components to Promise Tracker', () => {

    it('adds a single component', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("simple"));
        expect(pt.getComponentNames()).toEqual(["simple"]);
    });

    it('adds multiple components', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("cc"));
        pt.addComponent(new Component("cb"));
        pt.addComponent(new Component("ca"));
        expect(pt.getComponentNames()).toEqual(["ca", "cb", "cc"]);
    });

    it('adds overlapping components', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("c", [], [new Behavior("ba")]));
        expect(pt.getComponentVariants("c").length).toEqual(1);
        pt.addComponent(new Component("c", [], [new Behavior("bb")]));
        expect(pt.getComponentVariants("c").length).toEqual(2);
        expect(pt.getComponentNames()).toEqual(["c"]);
        expect(pt.getBehaviorNames()).toEqual(["ba", "bb"]);
    });

    it('doesnt add identical component', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("c", [], [new Behavior("b")]));
        expect(pt.getComponentVariants("c").length).toEqual(1);
        pt.addComponent(new Component("c", [], [new Behavior("b")]));
        expect(pt.getComponentVariants("c").length).toEqual(1);
    });

});
