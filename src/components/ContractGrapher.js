import React from "react";
import { useState, useEffect } from 'react';
import Mermaid from './Mermaid';
import { Form } from 'react-bootstrap';
import { allFromYAML } from '../libs/promise-tracker/contract';
import PromiseTracker from '../libs/promise-tracker/promise-tracker';
import ptdiagram from '../libs/promise-tracker/diagram';

export default function ContractGrapher({contracts}) {
    const [diagram, setDiagram] = useState("sequenceDiagram\nyou->>contract: enter something");
    const [dComponent, setDComponent] = useState("");
    const [dBehavior, setDBehavior] = useState("");
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
            if (dComponent === null || dComponent === "") {
                setDiagram("sequenceDiagram\nyou->>component: enter something");
                return;
            }
            if (dBehavior === null || dBehavior === "") {
                setDiagram("sequenceDiagram\nyou->>behavior: enter something");
                return;
            }
            if (!pt.getBehaviorNames().includes(dBehavior)) {
                setDiagram("sequenceDiagram\nyou->>behavior: enter a valid behacvior");
                return;
            }
            setDiagram(ptdiagram({...pt.resolve(dBehavior), component: dComponent}));
            } catch {console.log("stuff")};
        }, [pt, dComponent, dBehavior]);
    
    const updateDComponent = (e) => {
        e.preventDefault();
        setDComponent(e.target.value);
    };
    
    const updateDBehavior = (e) => {
        e.preventDefault();
        setDBehavior(e.target.value);
    };    

    return <>
            <Form>
              <Form.Control type="text" placeholder="Component" value={dComponent} onChange={updateDComponent} />
              <Form.Control type="text" placeholder="Behavior" value={dBehavior} onChange={updateDBehavior} />
            </Form>
            <Mermaid chart={diagram}></Mermaid>
        </>
}