import React, { useState, useEffect } from "react";
import ContractText from './ContractText';
import ContractGraph from './ContractGraph';
import PromiseNetworkGraph from './PromiseNetworkGraph';
import { Form, Tabs, Tab } from 'react-bootstrap';
import { get_pt } from '../wptpkg';

export default function ContractGrapher({ initDone, contracts, simulations }) {
    const [dComponent, setDComponent] = useState("---");
    const [dBehavior, setDBehavior] = useState("---");
    const [pt, setPt] = useState(null);
    // const [pt, setPt] = useState(new PromiseTracker());
    const [sims, setSims] = useState({});
    const [activeView, setActiveView] = useState("overview");

    useEffect(() => {
        const toHandler = setTimeout(() => {
            try {
                if (contracts.length === 0) {
                    setPt(null);
                    setSims({});
                    setDComponent("---");
                    setDBehavior("---");
                    return;
                }
                if (contracts.filter((c) => c.err).length > 0) {
                    return;
                }
                if (!initDone) {
                    return;
                }
                // const npt = new PromiseTracker();
                const npt = get_pt();
                const nsims = {};
                simulations.forEach((s) => {
                    nsims[s] = get_pt();
                });
                contracts.forEach((c) => {
                    if (c.text) {
                        npt.add_contract(c.text);
                        [...c.sims].filter((s) => simulations.includes(s)).forEach((s) => nsims[s].add_contract(c.text));
                    };
                });
                setPt(npt);
                setSims(nsims);
            } catch (e) {
                console.log(e);
            };
        }, 500);
        return () => {
            clearTimeout(toHandler);
        };
    }, [initDone, contracts, simulations]);

    const updateDComponent = (e) => {
        e.preventDefault();
        setDComponent(e.target.value);
        setDBehavior("---");
    };

    const updateDBehavior = (e) => {
        e.preventDefault();
        setDBehavior(e.target.value);
    };

    let wants = [];
    let wantsValid = false;
    if (dComponent !== "---" && pt && pt.has_agent(dComponent)) {
        const behaviorOptions = pt.get_agent_wants(dComponent);
        if (behaviorOptions.length === 0) {
            wants = [{ value: "---", display: "This component has no wants entries" }];
        } else {
            wants = [
                { value: "---", display: "Select a behavior" },
                ...behaviorOptions.map((b) => { return { value: b, display: b } })
            ];
            wantsValid = true;
        };
    } else {
        wants = [{ value: "---", display: "Select Component First" }];
    };

    let components = pt ? pt.get_agent_names() : [];

    return <>
        <Tabs activeKey={activeView} onSelect={(k) => setActiveView(k || "overview")} className="mb-3">
            <Tab eventKey="overview" title="Overview">
                <div style={{ marginTop: '1rem' }}>
                    <div style={{ marginBottom: '1rem' }}>
                        <h3 style={{ margin: 0 }}>Promise Relationships Overview</h3>
                        <p style={{ color: '#666', fontSize: '0.9em', marginTop: '0.25rem', marginBottom: 0 }}>
                            This view shows all components and their promise relationships.
                        </p>
                    </div>
                    <div style={{ display: 'flex', gap: '1rem', marginTop: '1rem' }}>
                        {simulations.map((s, i) => (
                            <div key={i} style={{ flex: 1, minWidth: 0, border: '1px solid #ccc', borderRadius: '4px', padding: '0.5rem', background: '#fafbfc' }}>
                                <div style={{ fontWeight: 'bold', marginBottom: '0.5rem', textAlign: 'center' }}>Simulation {s}</div>
                                <PromiseNetworkGraph pt={sims[s]} simId={`network-${s}`} />
                            </div>
                        ))}
                    </div>
                </div>
            </Tab>
            <Tab eventKey="detailed" title="Detailed View">
                <div style={{ marginTop: '1rem' }}>
                    <h3>Detailed Promise Resolution</h3>
                    <p style={{ color: '#666', fontSize: '0.9em', marginBottom: '1rem' }}>
                        Select a component and behavior to see how promises are resolved.
                    </p>
                    <Form>
                        <Form.Select onChange={updateDComponent} value={dComponent}>
                            <option value="---" >Select a Component</option>
                            {components.map((c, i) =>
                                <option key={i} value={c}>{c}</option>
                            )}
                        </Form.Select>
                        <Form.Select onChange={updateDBehavior} disabled={!wantsValid} value={dBehavior} style={{ marginTop: '0.5rem' }}>
                            {wants.map((w, i) =>
                                <option key={i} value={w.value}>{w.display}</option>
                            )}
                        </Form.Select>
                    </Form>
                    <div style={{ display: 'flex', gap: '1rem', marginTop: '1rem' }}>
                        {simulations.map((s, i) => (
                            <div key={i} style={{ flex: 1, minWidth: 0, border: '1px solid #ccc', borderRadius: '4px', padding: '0.5rem', background: '#fafbfc' }}>
                                <div style={{ fontWeight: 'bold', marginBottom: '0.5rem', textAlign: 'center' }}>Simulation {s}</div>
                                <Tabs defaultActiveKey="text" className="mb-2">
                                    <Tab eventKey="text" title="Text View">
                                        <ContractText pt={sims[s]} selectedComponent={dComponent} selectedBehavior={dBehavior}/>
                                    </Tab>
                                    <Tab eventKey="sequence" title="Sequence Diagram">
                                        <ContractGraph simId={`seq-${s}`} pt={sims[s]} selectedComponent={dComponent} selectedBehavior={dBehavior}/>
                                    </Tab>
                                </Tabs>
                            </div>
                        ))}
                    </div>
                </div>
            </Tab>
        </Tabs>
    </>
}
