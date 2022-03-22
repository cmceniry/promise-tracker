import React from "react";
import { useState, useEffect } from 'react';
import Mermaid from './Mermaid';
import { Form } from 'react-bootstrap';
import { allFromYAML } from '../libs/promise-tracker/contract';
import PromiseTracker from '../libs/promise-tracker/promise-tracker';
import ptdiagram from '../libs/promise-tracker/diagram';

export default function ContractGrapher({contracts}) {
    const [diagram, setDiagram] = useState("sequenceDiagram\nyou->>contract: enter something");
    const [dComponent, setDComponent] = useState("---");
    const [dBehavior, setDBehavior] = useState("---");
    const [pt, setPt] = useState(new PromiseTracker());

    useEffect(() => {
        try {
            if (contracts.length === 0) {
                setDiagram("sequenceDiagram\nyou->>contract: enter something");
                return;
            }
            if (contracts.filter((c) => c.err).length > 0) {
                return;
            }
            const npt = new PromiseTracker();
            contracts.forEach((c) => {
                if (c.text) {
                    allFromYAML(c.text).forEach((comp) => npt.addComponent(comp));
                }
            });
            setPt(npt);
        } catch {};
    }, [contracts]);

    useEffect(() => {
        try {
            if (!pt) {
                setDiagram("sequenceDiagram\nyou->>contract: enter something");
                return;
            }
            if (dComponent === null || dComponent === "---") {
                setDiagram("sequenceDiagram\nyou->>component: select component");
                return;
            }
            if (dBehavior === null || dBehavior === "---") {
                setDiagram("sequenceDiagram\nyou->>behavior: select behavior");
                return;
            }
            if (!pt.getBehaviorNames().includes(dBehavior)) {
                setDiagram("sequenceDiagram\nyou->>behavior: enter a valid behavior");
                return;
            }
            setDiagram(ptdiagram({...pt.resolve(dBehavior), component: dComponent}));
            } catch {console.log("rendering failed")};
        }, [pt, dComponent, dBehavior]);
    
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
    if (pt && dComponent !== "---") {
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
        <Mermaid chart={diagram}></Mermaid>
    </>
}
