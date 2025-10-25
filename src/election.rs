use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};
use tonic::{Request, Response, Status};
use async_trait::async_trait;

use crate::election_service::{
    bully_server::Bully,
    bully_client::BullyClient,
    ElectionRequest,
    ElectionResponse,
    Coordinator,
    PingRequest,
    PingResponse,
    Node as ProtoNode,
};

/// A simple representation of a peer node.
#[derive(Clone, Debug)]
pub struct Node {
    pub id: i32,
    pub addr: String,
}

impl From<&Node> for ProtoNode {
    fn from(n: &Node) -> Self {
        ProtoNode { id: n.id, addr: n.addr.clone() }
    }
}

/// Election manager implementing a simplified Bully algorithm.
#[derive(Clone)]
pub struct ElectionManager {
    self_node: Node,
    peers: Vec<Node>,
    leader: Arc<RwLock<Option<Node>>>,
}

impl ElectionManager {
    /// Create a new election manager.
    /// `self_node` is this node's id and address. `peers` is the list of other nodes in the cluster.
    pub fn new(self_node: Node, peers: Vec<Node>) -> Self {
        Self { self_node, peers, leader: Arc::new(RwLock::new(None)) }
    }

    /// Start a local election: contact higher-id peers and wait for any OK response.
    /// If no higher-id peer responds within the timeout, the manager declares itself leader and announces it.
    pub async fn start_election(&self) {
        let higher: Vec<Node> = self.peers.iter().filter(|p| p.id > self.self_node.id).cloned().collect();

        // If there are no higher nodes, immediately become leader.
        if higher.is_empty() {
            self.declare_leader().await;
            return;
        }

        // Contact higher nodes. If any responds OK, we back off.
        let mut someone_alive = false;
        for peer in higher.iter() {
            let addr = peer.addr.clone();
            let mut client = match timeout(Duration::from_secs(2), BullyClient::connect(format!("http://{}", addr))).await {
                Ok(Ok(c)) => c,
                _ => continue,
            };

            let req = tonic::Request::new(ElectionRequest { from: Some((&self.self_node).into()) });
            match timeout(Duration::from_secs(2), client.election(req)).await {
                Ok(Ok(resp)) => {
                    if resp.into_inner().ok {
                        someone_alive = true;
                        break;
                    }
                }
                _ => continue,
            }
        }

        if someone_alive {
            // A higher-id node is alive and will take over; wait for coordinator announcement (not implemented: passive wait)
            // For simplicity we do nothing here; a production implementation would subscribe/listen for coordinator announcements.
        } else {
            // No higher-id nodes responded: become leader
            self.declare_leader().await;
        }
    }

    async fn declare_leader(&self) {
        let leader_node = self.self_node.clone();
        *self.leader.write().await = Some(leader_node.clone());

        // Announce to all peers (best-effort)
        for peer in self.peers.iter() {
            let addr = peer.addr.clone();
            let leader = Coordinator { leader: Some((&leader_node).into()) };
            // fire-and-forget: try to connect and announce; ignore any errors
            tokio::spawn(async move {
                if let Ok(mut client) = BullyClient::connect(format!("http://{}", addr)).await {
                    let _ = client.announce_coordinator(tonic::Request::new(leader)).await;
                }
            });
        }
    }

    /// Get current leader (if any)
    pub async fn get_leader(&self) -> Option<Node> {
        self.leader.read().await.clone()
    }
}

/// Implement the server-side RPCs used by Bully algorithm.
#[derive(Clone)]
pub struct ElectionService { pub manager: ElectionManager }

#[async_trait::async_trait]
impl Bully for ElectionService {
    /// Handle incoming election messages from lower-id nodes.
    async fn election(&self, request: Request<ElectionRequest>) -> Result<Response<ElectionResponse>, Status> {
        let from = request.into_inner().from.ok_or_else(|| Status::invalid_argument("missing from"))?;
        // If the incoming node has lower id, we reply ok and start our own election.
        let ok = from.id < self.manager.self_node.id;
        if ok {
            // spawn our own election process because we are higher
            let mgr = self.manager.clone();
            tokio::spawn(async move { mgr.start_election().await });
        }
        Ok(Response::new(ElectionResponse { ok }))
    }

    /// Receive coordinator announcements
    async fn announce_coordinator(&self, request: Request<Coordinator>) -> Result<Response<PingResponse>, Status> {
        if let Some(leader) = request.into_inner().leader {
            let node = Node { id: leader.id, addr: leader.addr };
            *self.manager.leader.write().await = Some(node);
        }
        Ok(Response::new(PingResponse { alive: true }))
    }

    /// Simple ping
    async fn ping(&self, _request: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        Ok(Response::new(PingResponse { alive: true }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Very small unit test just ensures the manager constructs and declares itself leader when no higher peers.
    #[tokio::test]
    async fn declares_self_leader_when_no_higher() {
        let self_node = Node { id: 10, addr: "127.0.0.1:50051".into() };
        let peers = vec![Node { id: 1, addr: "127.0.0.1:50052".into() }];
        let mgr = ElectionManager::new(self_node.clone(), peers);
        mgr.start_election().await;
        let leader = mgr.get_leader().await;
        assert!(leader.is_some());
        assert_eq!(leader.unwrap().id, self_node.id);
    }
}
