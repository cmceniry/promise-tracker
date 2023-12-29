import React, { useState, useEffect } from "react";
import ContractGraph from './ContractGraph';
import { Form } from 'react-bootstrap';
import Tab from 'react-bootstrap/Tab';
import Tabs from 'react-bootstrap/Tabs';
import init, { get_pt } from '../wptpkg';

export default function ContractGrapher({ contracts, simulations }) {
    const [dComponent, setDComponent] = useState("---");
    const [dBehavior, setDBehavior] = useState("---");
    const [initDone, setInitDone] = useState(false);
    const [pt, setPt] = useState(null);
    // const [pt, setPt] = useState(new PromiseTracker());
    const [sims, setSims] = useState({});

    useEffect(() => {
        (async function () {
            await init();
            setPt(await get_pt());
            setInitDone(true);
        })();
    }, []);

    useEffect(() => {
        const toHandler = setTimeout(() => {
            try {
                if (contracts.length === 0) {
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
        <Form>
            <Form.Select onChange={updateDComponent}>
                <option value="---" >Select a Component</option>
                {components.map((c, i) =>
                    <option key={i} value={c}>{c}</option>
                )}
                {/* {console.log(typeof (components))} */}
            </Form.Select>
            <Form.Select onChange={updateDBehavior} disabled={!wantsValid}>
                {wants.map((w, i) =>
                    <option key={i} value={w.value}>{w.display}</option>
                )}
            </Form.Select>
        </Form>
        <Tabs>
            {simulations.map((s, i) => {
                return <Tab title={s} key={i} eventKey={i}>
                    <ContractGraph simId={s} pt={sims[s]} selectedComponent={dComponent} selectedBehavior={dBehavior} />
                </Tab>
            })}
        </Tabs>
    </>
}
