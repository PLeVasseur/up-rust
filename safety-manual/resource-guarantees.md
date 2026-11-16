# A resource guarantee the item relies on is required by name

`uman~sdk-aou-resource-guarantees~1`

Covers:
- aou~seooc-resource-guarantees~1

## Status and scope

Draft integrator guidance against the corrected specification input. The quotation states
requirements and assumptions, including future behavior; it is not an assertion
that the current SDK implements them.

- This entry binds only where the item relies on panic-free cleanup, non-waiting acquisition, non-waiting release or recoverable exhaustion on a serialized route.
- Its text was the "Owning wires" clause of the qualified-bindings-and-wires assumption at revision 6, and its wording is unchanged.
- Ordinary use of a wire that owns its values, heap-backed ones included, requires none of these guarantees and needs nothing from this entry.
- Guarantee admission, its named refusal and its reports are specified. This manual does not claim that they are implemented.

Read [manual status](README.md) before using this entry. A documentary coverage
link neither validates the deployment nor closes an implementation requirement.

## Assumption (verbatim)

<!-- aou-statement:start -->
> *Owning wires.*
> A wire that decodes into values that own what they hold, heap memory included, is ordinary on a serialized route that declares its allocation, and its ownership needs nothing further from the integrator.
>
> *Requiring a guarantee.*
> Where the item relies on panic-free cleanup, non-waiting acquisition, non-waiting release or recoverable exhaustion on such a route, the integrator *SHALL* require that guarantee in the route's declaration, and uses a supported configuration whose package establishes it for the exact type, wires, binding and allocation provider (xref:heap.adoc#req-ssr-heap-guarantees[A stronger resource guarantee is required by name]).
>
> *A guarantee that is not required.*
> A route that does not require a guarantee is promised none, whether or not evidence establishes it: its report shows that guarantee as not required and not admitted, and states separately whether it is established.
<!-- aou-statement:end -->

## Integrator actions

The integrator SHALL complete and retain the following checklist for the item:

- [ ] **Require the resource guarantees the item relies on.**
  - A wire that owns its values, heap-backed ones included, is ordinary on a serialized route that declares its allocation. That ownership gives no guarantee about how its cleanup behaves.
  - Where the item relies on panic-free cleanup, non-waiting acquisition, non-waiting release or recoverable exhaustion on a route, require it in that route's declaration.
  - Each guarantee is its own request: requiring one requires none of the others.
- [ ] **Read the startup report.**
  - Each guarantee is shown as required or not, established or not, and admitted or not. Rely only on a guarantee shown as admitted.
  - A guarantee shown as established and not required is evidence that the configuration has, not a promise to the item. Require it before relying on it.
  - Treat "not established" as absent: the evidence is missing or insufficient, whatever the route requires.
  - Do not read an integrity level, a strict-mode state or the name of a wire or provider as a request or as evidence.

## Violation manifestations

- A route that relies on a resource guarantee it did not require can wait in its allocator, end the process on exhaustion or panic in a destructor, with no refusal at setup.

## Detection limits

- The SDK refuses a required guarantee that is not established. It cannot tell that the item relies on one that no route requires.
- A binding's deferred return of a retained transport owner is a duty of the binding on every route. Requiring no guarantee does not remove it.
