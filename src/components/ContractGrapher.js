import React from "react";
import { useState, useEffect } from 'react';
import ContractGraph from './ContractGraph';
import { Form } from 'react-bootstrap';
import { allFromYAML } from '../libs/promise-tracker/contract';
import PromiseTracker from '../libs/promise-tracker/promise-tracker';

export default function ContractGrapher({contracts, simulations}) {
    const [dComponent, setDComponent] = useState("---");
    const [dBehavior, setDBehavior] = useState("---");
    const [pt, setPt] = useState(new PromiseTracker());
    const [sims, setSims] = useState({});

    useEffect(() => {
        const toHandler = setTimeout(() => {
            try {
                if (contracts.length === 0) {
                    return;
                }
                if (contracts.filter((c) => c.err).length > 0) {
                    return;
                }
                const npt = new PromiseTracker();
                const nsims = {};
                simulations.forEach((s) => {
                    nsims[s] = new PromiseTracker();
                });
                contracts.forEach((c) => {
                    if (c.text) {
                        const allComponents = allFromYAML(c.text);
                        allComponents.forEach((comp) => {
                            npt.addComponent(comp);
                            [...c.sims].filter((s) => simulations.includes(s)).forEach((s) => nsims[s].addComponent(comp));
                        });
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
    }, [contracts, simulations]);

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
    if (pt && dComponent !== "---" && pt.Components.get(dComponent)) {
        const behaviorOptions = pt.Components.get(dComponent).map((c) => c.getWants().map((b) => b.name)).flat();
        if (behaviorOptions.length === 0) {
            wants =  [{value: "---", display: "This component has no wants entries"}];
        } else {
            wants = [
                {value: "---", display: "Select a behavior"},
                ...behaviorOptions.map((b) => {return {value: b, display: b}})
            ];
            wantsValid = true;
        };
    } else {
        wants = [{value: "---", display: "Select Component First"}];
    };

    return <>
        <Form>
            <Form.Select onChange={updateDComponent}>
                <option value="---" >Select a Component</option>
                {pt.getComponentNames().map((cName, i) =>
                    <option key={i} value={cName}>{cName}</option>
                )}
            </Form.Select>
            <Form.Select onChange={updateDBehavior} disabled={!wantsValid}>
                {wants.map((w,i) =>
                    <option key={i} value={w.value}>{w.display}</option>
                )}
            </Form.Select>
        </Form>
        {simulations.map((s, i) => {
            return <ContractGraph key={i} simId={s} pt={sims[s]} selectedComponent={dComponent} selectedBehavior={dBehavior}/>
        })}
    </>
}
