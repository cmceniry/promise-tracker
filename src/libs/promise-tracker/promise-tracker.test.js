import { Behavior, Collective, Component, compareBehavior } from './contract.js';
import PromiseTracker from './promise-tracker.js';

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

describe('Adding Collectives to Promise Tracker', () => {
    it('adds a single collective', () => {
        const pt = new PromiseTracker();
        pt.add(new Collective("l1"));
        expect(pt.getCollectiveNames()).toEqual(["l1"]);
        expect(pt.getCollectiveComponents()).toEqual([]);
    });

    it('add multiple collectives', () => {
        const pt = new PromiseTracker();
        pt.add(new Collective("l1", ["c1", "c2"]));
        pt.add(new Collective("l2", ["c3", "c4"]));
        expect(pt.getCollectiveNames()).toEqual(["l1", "l2"]);
        expect(pt.getCollectiveComponents()).toEqual(["c1", "c2", "c3", "c4"]);
    });

    it('adds collective after components', () => {
        const pt = new PromiseTracker();
        pt.add(new Component("c1", [], [new Behavior("b1")]));
        pt.add(new Component("c2", [], [new Behavior("b2")]));
        pt.add(new Component("c3", [], [new Behavior("b3")]));
        pt.add(new Collective("l1", ["c1", "c2"]));
        expect(pt.getComponentNames()).toEqual(["c3", "l1"]);
        expect(pt.getCollectiveByComponentName("c1").name).toEqual("l1");
        expect(pt.getCollectiveByComponentName("c3")).toEqual(undefined);
    });

    it('add collective between components', () => {
        const pt = new PromiseTracker();
        pt.add(new Component("c1", [], [new Behavior("b1")]));
        pt.add(new Collective("l1", ["c1", "c2"]));
        expect(pt.getComponentNames()).toEqual(["l1"]);
        pt.add(new Component("c2", [], [new Behavior("b2")]));
        expect(pt.getComponentNames()).toEqual(["l1"]);
        pt.add(new Component("c3", [], [new Behavior("b3")]));
        expect(pt.getComponentNames()).toEqual(["c3", "l1"]);
    });

    it('add collective with overlapping components', () => {
        const pt = new PromiseTracker();
        pt.add(new Component("c1", [], [new Behavior("b1")]));
        pt.add(new Component("c1", [], [new Behavior("b2")]));
        pt.add(new Collective("l1", ["c1", "c2"]));
        expect(pt.getComponentNames()).toEqual(["l1"]);
        expect(pt.getBehaviorNames()).toEqual(["b1", "b2"]);
        pt.add(new Component("c1", [], [new Behavior("b3")]));
        pt.add(new Component("c2", [], [new Behavior("b4")]));
        expect(pt.getBehaviorNames()).toEqual(["b1", "b2", "b3", "b4"]);
    });

    it('collective behavior providers', () => {
        const pt = new PromiseTracker();
        pt.add(new Component("c1", [], [new Behavior("b1")]));
        pt.add(new Component("c2", [], [new Behavior("b2")]));
        pt.add(new Component("c3", [], [new Behavior("b3", ["b2"])]));
        pt.add(new Collective("l1", ["c3"]));
        expect(pt.getBehaviorProviders("b1")).toEqual([
            {behavior: new Behavior("b1"), componentName: "c1"}
        ]);
        expect(pt.getBehaviorProviders("b3")).toEqual([
            {behavior: new Behavior("b3", ["b2"]), componentName: "l1"}
        ]);
    });

    it('handling collective with instances', () => {
        const pt = new PromiseTracker();
        pt.add(new Component("c1", [], [new Behavior("b1", ["b2"])]));
        pt.add(new Component("c2", [], [new Behavior("b2", ["b3", "b4", "b5"])]));
        pt.add(new Component("c3", [], [new Behavior("b3")]));
        pt.add(new Collective("l1", [], [
            {
                "name": "i1",
                "components": ["c1", "c2", "c3", "c4"],
                "providesTag": "pt1",
                "conditionsTag": "ct1",
            },
            {
                "name": "i2",
                "components": ["c1", "c2", "c3", "c4"],
                "providesTag": "pt2",
                "conditionsTag": "ct2",
            },
        ]));
        pt.add(new Component("c4", [], [new Behavior("b4")]));
        pt.add(new Component("c5", [], [new Behavior("b5 | ct1")]));
        expect(pt.getBehaviorNames()).toEqual([
            "b1 | pt1", "b1 | pt2",
            "b2 | pt1", "b2 | pt2",
            "b3 | pt1", "b3 | pt2",
            "b4 | pt1", "b4 | pt2",
            "b5 | ct1", "b5 | ct2",
        ]);
        expect(pt.getBehaviorProviders("b1 | pt1")).toEqual([
            {behavior: new Behavior("b1 | pt1", ["b5 | ct1"]), componentName: "i1"},
        ]);
        expect(pt.getBehaviorProviders("b3 | pt1")).toEqual([
            {behavior: new Behavior("b3 | pt1"), componentName: "i1"},
        ]);
        expect(pt.getBehaviorProviders("b5 | ct1")).toEqual([
            {behavior: new Behavior("b5 | ct1"), componentName: "c5"},
        ]);
        expect(pt.getBehaviorProviders("b1 | pt2")).toEqual([
            {behavior: new Behavior("b1 | pt2", ["b5 | ct2"]), componentName: "i2"},
        ]);
        expect(pt.resolve("b1 | pt1")).toEqual(
            {behavior: "b1 | pt1", satisfied: [
                {component: "i1", conditions: [
                    {behavior: "b5 | ct1", satisfied: [
                        {component: "c5"},
                    ]},
                ]},
            ]},
        );
        expect(pt.resolve("b3 | pt1")).toEqual(
            {behavior: "b3 | pt1", satisfied: [
                {component: "i1"},
            ]},
        );
        expect(pt.resolve("b2 | pt2")).toEqual(
            {behavior: "b2 | pt2", unsatisfied: [
                {component: "i2", conditions: [
                    {behavior: "b5 | ct2", unsatisfied: []},
                ]},
            ]},
        );
    });
});

describe('full resolving', () => {

    it('passed OR with unsatisifed parts', () => {
        const pt = new PromiseTracker();
        pt.addComponent(new Component("c1", [], [
            new Behavior("b1", ["b2"]),
            new Behavior("b1", ["b3", "b4"]),
        ]));
        pt.addComponent(new Component("c2", [], [
            new Behavior("b2"),
        ]));
        pt.addComponent(new Component("c3", [], [
            new Behavior("b3"),
        ]));
        expect(pt.fullResolve("b1")).toEqual({
            behavior: "b1",
            satisfied: [
                {
                    component: "c1",
                    conditions: [
                        {
                            behavior: "b2",
                            satisfied: [
                                {
                                    component: "c2",
                                }
                            ],
                        },
                    ],
                },
            ],
            unsatisfied: [
                {
                    component: "c1",
                    conditions: [
                        {
                            behavior: "b3",
                            satisfied: [
                                {
                                    component: "c3",
                                },
                            ],
                        },
                        {
                            behavior: "b4",
                            unsatisfied: [],
                        },
                    ],
                },
            ],
        });
    });

});

describe('pruned resolving', () => {

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

describe('prune resolves', () => {

    it('1 level - satisfied', () => {
        const pt = new PromiseTracker();
        expect(pt.pruneResolve({
            behavior: "b1",
            satisfied: [
                {
                    component: "c1",
                },
                {
                    component: "c2",
                },
            ],
        })).toEqual({
            behavior: "b1",
            satisfied: [
                {
                    component: "c1",
                },
                {
                    component: "c2",
                },
            ],
        })
    });

    it('1 level - satisfied and unsatisfied', () => {
        const pt = new PromiseTracker();
        expect(pt.pruneResolve({
            behavior: "b1",
            satisfied: [
                {
                    component: "c1",
                },
            ],
            unsatisfied: [
                {
                    component: "c2",
                    conditions: [
                        {
                            behavior: "b2",
                            unsatisfied: [],
                        },
                    ],
                },
            ],
        })).toEqual({
            behavior: "b1",
            satisfied: [
                {
                    component: "c1",
                },
            ],
        });
    });

    it('1 level - empty unsatisfied', () => {
        const pt = new PromiseTracker();
        expect(pt.pruneResolve({
            behavior: "b1",
            unsatisfied: [],
        })).toEqual({
            behavior: "b1",
            unsatisfied: [],
        });
    });

    it('2 level - unsatisfied', () => {
        const pt = new PromiseTracker();
        expect(pt.pruneResolve({
            behavior: "b1",
            unsatisfied: [
                {
                    component: "c2",
                    conditions: [
                        {
                            behavior: "b2",
                            unsatisfied: [],
                        },
                    ],
                },
            ],
        })).toEqual({
            behavior: "b1",
            unsatisfied: [
                {
                    component: "c2",
                    conditions: [
                        {
                            behavior: "b2",
                            unsatisfied: [],
                        },
                    ],
                },
            ],
        });

    });

});

describe('collective resolving', () => {
    it('resolve collective', () => {
        const pt = new PromiseTracker();
        pt.add(new Collective("l1", ["c1", "c2"]));
        pt.addComponent(new Component("c1", [], [new Behavior("b1")]));
        pt.addComponent(new Component("c2", [], [new Behavior("b2")]));
        pt.addComponent(new Component("c3", [], [new Behavior("b3", ["b1"])]));
        expect(pt.resolve("b3")).toEqual(
            {behavior: "b3", satisfied: [
                {component: "c3", conditions: [
                    {behavior: "b1", satisfied: [
                        {component: "l1"},
                    ]},
                ]},
            ]},
        );
    });
});
