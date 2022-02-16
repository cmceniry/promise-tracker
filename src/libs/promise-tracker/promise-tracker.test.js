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

describe('resolving', () => {

    it('getBehaviorProviders', () => {
        const pt = new PromiseTracker();
        const c1 = new Component("c1", [new Behavior("b2")], []);
        pt.addComponent(c1);
        const c2 = new Component("c2", [], [new Behavior("b2")]);
        pt.addComponent(c2);
        const c3 = new Component("c3", [], [new Behavior("b3a"), new Behavior("b3b")]);
        pt.addComponent(c3);
        const c4 = new Component("c4", [], [new Behavior("b4", ["cond1"])]);
        pt.addComponent(c4);
        const c6 = new Component("c6", [], [new Behavior("b56")]);
        const c5 = new Component("c5", [], [new Behavior("b56")]);
        pt.addComponent(c6);
        pt.addComponent(c5);
        const c7 = new Component("c7", [], [
            new Behavior("b7", ["cond7a"]),
            new Behavior("b7", ["cond7b"]),
        ]);
        pt.addComponent(c7);
        expect(pt.getBehaviorProviders("b1")).toEqual([]);
        expect(pt.getBehaviorProviders("b2")).toEqual([{componentName: "c2", behavior: c2.provides[0]}]);
        expect(pt.getBehaviorProviders("b3a")).toEqual([{componentName: "c3", behavior: c3.provides[0]}]);
        expect(pt.getBehaviorProviders("b3b")).toEqual([{componentName: "c3", behavior: c3.provides[1]}]);
        expect(pt.getBehaviorProviders("b4")).toEqual([{componentName: "c4", behavior: c4.provides[0]}]);
        expect(pt.getBehaviorProviders("b56")).toEqual([
            {componentName: "c5", behavior: c5.provides[0]},
            {componentName: "c6", behavior: c6.provides[0]},
        ]);
        expect(pt.getBehaviorProviders("b7")).toEqual([
            {componentName: "c7", behavior: c7.provides[0]},
            {componentName: "c7", behavior: c7.provides[1]},
        ]);
    });

    it('does a simple resolve', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("c1", [new Behavior("b2")], []));
        pt.addComponent(new Component("c2", [], [new Behavior("b2")]));
        expect(pt.resolve("b2")).toEqual({behavior: "b2", satisfied: [{component: "c2"}]});
    });

    it('does a simple unresolve', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("c1", [new Behavior("b1")], []));
        pt.addComponent(new Component("c2", [new Behavior("b2")], [new Behavior("b1")]));
        expect(pt.resolve("b2")).toEqual({behavior: "b2", unsatisfied: []});
    });

    it('does a recursive satisfied', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("c1", [new Behavior("b1")], []));
        pt.addComponent(new Component("c2", [], [new Behavior("b1", ["b2"])]));
        pt.addComponent(new Component("c3", [], [new Behavior("b2")]));
        expect(pt.resolve("b1")).toEqual({
            behavior: "b1",
            satisfied: [
                {
                    component: "c2",
                    conditions: [
                        {
                            behavior: "b2",
                            satisfied: [
                                {
                                    component: "c3",
                                },
                            ],
                        },
                    ],
                },
            ],
        });
    });

    it('fails a recursive unsatisfied', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("c1", [new Behavior("b1")], []));
        pt.addComponent(new Component("c2", [], [new Behavior("b1", ["b2"])]));
        pt.addComponent(new Component("c3", [], [new Behavior("b2", ["b3"])]));
        expect(pt.resolve("b1")).toEqual({
            behavior: "b1",
            unsatisfied: [
                {
                    component: "c2",
                    conditions: [
                        {
                            behavior: "b2",
                            unsatisfied: [
                                {
                                    component: "c3",
                                    conditions: [
                                        {
                                            behavior: "b3",
                                            unsatisfied: [],
                                        },
                                    ],
                                },
                            ],
                        },
                    ],
                },
            ],
        });
    });

    it('fails a double conditional (AND) unsatisfied', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("c1", [], [
            new Behavior("b1", ["b2", "b3"]),
        ]));
        expect(pt.resolve("b1")).toEqual({
            behavior: "b1",
            unsatisfied: [
                {
                    component: "c1",
                    conditions: [
                        {
                            behavior: "b2",
                            unsatisfied: [],
                        },
                        {
                            behavior: "b3",
                            unsatisfied: [],
                        },
                    ],
                },
            ],
        });
    });

    it('fails a double provides (OR) unsatisfied', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("c1", [], [
            new Behavior("b1", ["b2"]),
            new Behavior("b1", ["b3"]),
        ]));
        expect(pt.resolve("b1")).toEqual({
            behavior: "b1",
            unsatisfied: [
                {
                    component: "c1",
                    conditions: [
                        {
                            behavior: "b2",
                            unsatisfied: [],
                        },
                    ],
                },
                {
                    component: "c1",
                    conditions: [
                        {
                            behavior: "b3",
                            unsatisfied: [],
                        },
                    ],
                },
            ],
        });
    });

    it('passes a good double-double recursion', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("c1", [], [
            new Behavior("b1", ["b2", "b3"]),
            new Behavior("b1", ["b4", "b5"]),
        ]));
        pt.addComponent(new Component("c3", [], [new Behavior("b3")]));
        pt.addComponent(new Component("c4", [], [new Behavior("b4")]));
        pt.addComponent(new Component("c5", [], [new Behavior("b5")]));
        expect(pt.resolve("b1")).toEqual({
            behavior: "b1",
            satisfied: [
                {
                    component: "c1",
                    conditions: [
                        {
                            behavior: "b4",
                            satisfied: [
                                {
                                    component: "c4",
                                },
                            ],
                        },
                        {
                            behavior: "b5",
                            satisfied: [
                                {
                                    component: "c5",
                                }
                            ],
                        },
                    ],
                },
            ],
        });
    });

    it('fails a bad double-double recursion', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("c1", [], [
            new Behavior("b1", ["b2", "b3"]),
            new Behavior("b1", ["b4", "b5"]),
        ]));
        pt.addComponent(new Component("c3", [], [new Behavior("b3")]));
        pt.addComponent(new Component("c4", [], [new Behavior("b4")]));
        expect(pt.resolve("b1")).toEqual({
            behavior: "b1",
            unsatisfied: [
                {
                    component: "c1",
                    conditions: [
                        {
                            behavior: "b2",
                            unsatisfied: [],
                        },
                        {
                            behavior: "b3",
                            satisfied: [
                                {
                                    component: "c3",
                                }
                            ],
                        },
                    ],
                },
                {
                    component: "c1",
                    conditions: [
                        {
                            behavior: "b4",
                            satisfied: [
                                {
                                    component: "c4",
                                },
                            ],
                        },
                        {
                            behavior: "b5",
                            unsatisfied: [],
                        },
                    ],
                },
            ],
        });
    });

});
