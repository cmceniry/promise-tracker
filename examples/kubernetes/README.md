# Kubernetes, as promises

The Kubernetes project modeled in the promise-tracker contract format: who
promises what to whom, and what each promise rests on. It is meant as a
large, realistic data set for the visualizer rather than as documentation of
Kubernetes, though the comments try to be accurate.

Modeled against the Kubernetes documentation as of October 2026 (the
kube-controller-manager reference listing 49 controllers, scheduler config
with PodGroup extension points, kube-proxy defaulting to iptables with
nftables announced as the future default).

## Files

| File | What it holds |
|---|---|
| `apiserver.yaml` | `kube-apiserver` as one Agent. Its request pipeline (authn, authz, admission, flow control) and the behaviours clients lean on (list/watch, leases, binding, eviction, CSRs, aggregation) are separate promises. |
| `controller-manager.yaml` | `kube-controller-manager` as a SuperAgent: four shared internals plus one Agent per controller in the `--controllers` list, alphabetical. |
| `scheduler.yaml` | `kube-scheduler` as one Agent, with queueing, filtering, scoring, binding and preemption as separate promises. Plugin names are in the comments. |
| `kubelet.yaml` | `kubelet` as one Agent: pod execution, node registration and heartbeats, the TLS bootstrap chain, its own API, resource management, and the plumbing a pod needs. |
| `etcd.yaml` | `etcd`. |
| `node-runtime.yaml` | `containerd`, `runc`, `cni-plugin`, `kube-proxy`, and the two halves of a CSI driver. |
| `addons.yaml` | `coredns`, and `cloud-controller-manager` as a SuperAgent of its three controllers. |
| `infrastructure.yaml` | The leaves: `cluster-pki`, `node-operating-system`, `cloud-provider`, `container-image-registry`. |
| `consumers.yaml` | The roots: `application-owner` and `cluster-administrator`, whose wants pull on everything above. |
| `cluster.yaml` | Node shapes (`control-plane-node`, `worker-node`) and six Instances of them. |

## Reading it

Promise names are capabilities in plain words, shared across files so the
chains connect. The spine is:

```
consumers  ->  controllers / scheduler / kubelet  ->  kubernetes-api, list-watch-streams
           ->  cluster-state-storage (etcd)  ->  certificates, disk, kernel
```

Dependencies are written as `conditions` on the promise that needs them, so
the graph shows why a promise holds. `wants` are kept for an agent's
standing needs on the outside world. The two overlap a little by design.

Two loads are worth trying:

- **The whole directory.** `cluster.yaml` turns the components into six node
  instances, three control-plane and three worker. Components fold into their
  node, so you see nodes, the consumers, and the cluster-wide services.
- **Everything except `cluster.yaml`.** One logical copy of each component.
  This is the view that shows the apiserver, the controller manager and the
  kubelet as their own nodes.

Promise names are not parameterized, so every node instance makes identical
promises and a want for `pod-execution` sees one offer per node. That is the
right answer to "is there anywhere to run this" and the wrong one to "is
worker-2 healthy". `docs/design/parameterized-behaviors.md` describes the
mechanism that would fix it.

## What is unsatisfied, on purpose

`horizontal-pod-autoscaling` conditions on `resource-metrics-api`, which
metrics-server would provide and nothing here does. Everything else resolves.

## Where the lines were drawn

- The apiserver, scheduler and kubelet are single Agents; only the controller
  managers are broken into their controllers.
- `cluster-dns` is a want of the kubelet, not a condition of `pod-execution`.
  The kubelet writes the kube-dns address into a pod's resolv.conf whether or
  not CoreDNS answers, and making it a condition is circular because CoreDNS
  records come from pod status.
- `node-pod-routes` has two providers, the CNI plugin (overlay or BGP) and the
  cloud route controller. Both become offers; that is the tracker's normal
  multi-provider reading.
- Three controllers are disabled by default upstream and say so in their
  comments; they are still modeled.
- Capacity, ordering and timing are out of scope. A promise is kept or not.

## Checking it

```
cargo run -p cli -- validate $(for f in examples/kubernetes/*.yaml; do echo -f $f; done)
cargo run -p cli -- check-unsatisfied -f examples/kubernetes
cargo run -p cli -- who-provides -f examples/kubernetes kube-apiserver
```

## Sources

- https://kubernetes.io/docs/concepts/architecture/
- https://kubernetes.io/docs/concepts/overview/components/
- https://kubernetes.io/docs/reference/command-line-tools-reference/kube-apiserver/
- https://kubernetes.io/docs/reference/command-line-tools-reference/kube-controller-manager/
- https://kubernetes.io/docs/reference/command-line-tools-reference/kubelet/
- https://kubernetes.io/docs/reference/scheduling/config/
- https://kubernetes.io/docs/concepts/architecture/nodes/
- https://kubernetes.io/docs/concepts/architecture/leases/
- https://kubernetes.io/docs/concepts/architecture/cri/
- https://kubernetes.io/docs/concepts/architecture/cloud-controller/
- https://kubernetes.io/docs/reference/networking/virtual-ips/
- https://kubernetes.io/docs/concepts/services-networking/dns-pod-service/
- https://kubernetes.io/docs/concepts/storage/volumes/
