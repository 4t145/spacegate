// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { K8sServiceData } from "./K8sServiceData";

export type BackendHost = { "kind": "Host", host: string, } | { "kind": "K8sService" } & K8sServiceData;