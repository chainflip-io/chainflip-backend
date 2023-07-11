#!/usr/bin/env bash

# Scale down all deployments and stateful sets to 0
kubectx arn:aws:eks:eu-central-1:505970484198:cluster/euc1-staging
read -p "🚨💣 WARNING 💣🚨 Do you want to delete all PVCs? [y/N] " DELETE_PVCS
kubectl scale --replicas=0 deployment --all --context arn:aws:eks:eu-central-1:505970484198:cluster/euc1-staging -n backspin
kubectl scale --replicas=0 statefulset --all --context arn:aws:eks:eu-central-1:505970484198:cluster/euc1-staging -n backspin

# Delete all PVCs
kubectl delete pvc --all --context arn:aws:eks:eu-central-1:505970484198:cluster/euc1-staging -n backspin

# Scale up all deployments and stateful sets to 1
kubectl scale --replicas=1 deployment --all --context arn:aws:eks:eu-central-1:505970484198:cluster/euc1-staging -n backspin
kubectl scale --replicas=1 statefulset --all --context arn:aws:eks:eu-central-1:505970484198:cluster/euc1-staging -n backspin

for i in bashful doc dopey ; do helm upgrade $i charts/chainflip-node -f /tmp/$i.yaml --install; done