use crate::cores::xray::is_xray_protocol;
use crate::cores::{extract_protocol_users, str_or_empty, CoreUser};
use mypanel_proto::links::{apply_vless_route, derive_ss2022_psk, is_ss2022};
use mypanel_proto::{InboundConfig, Protocol, RoutingRuleConfig};
use prost::Message;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tonic::client::Grpc;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::transport::Endpoint;

/// Single incremental user operation (`AddUser` or `RemoveUser`) for Xray `HandlerService`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserOp {
    Add {
        inbound_tag: String,
        protocol: Protocol,
        user: CoreUser,
        cipher_or_method: String,
    },
    Remove {
        inbound_tag: String,
        email: String,
    },
}

/// Diff outcome between the currently running configuration and a newly requested configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigDiff {
    /// Identical effective configuration — no action required.
    NoChange,
    /// Only Xray inbound user lists changed — can be applied online via `HandlerService` hot-reload.
    HotReload(Vec<UserOp>),
    /// Structural settings, non-Xray cores, ports, streams, or routing rules changed — requires full apply.
    FullRestart(String),
}

/// Trait abstracting Xray gRPC `HandlerService` (`AlterInbound`) and `StatsService` (`QueryStats` / `GetAllOnlineUsers`)
/// so unit tests can mock gRPC calls deterministically while production uses tonic/CLI against `127.0.0.1:10085`.
pub trait XrayHandlerClient: Send + Sync {
    /// Applies a batch of incremental [`UserOp`] operations (`AddUser` / `RemoveUser`).
    fn apply_ops<'a>(
        &'a self,
        ops: &'a [UserOp],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>>;
}

/// Compares `old_inbounds` + `old_rules` with `new_inbounds` + `new_rules` to determine whether
/// an incremental Xray `HandlerService` hot-reload is possible.
pub fn diff_configs(
    old_inbounds: &[InboundConfig],
    old_rules: &[RoutingRuleConfig],
    new_inbounds: &[InboundConfig],
    new_rules: &[RoutingRuleConfig],
) -> Result<ConfigDiff, String> {
    if old_rules != new_rules {
        return Ok(ConfigDiff::FullRestart("Global routing rules changed".into()));
    }

    let old_enabled: Vec<&InboundConfig> = old_inbounds.iter().filter(|i| i.enable).collect();
    let new_enabled: Vec<&InboundConfig> = new_inbounds.iter().filter(|i| i.enable).collect();

    if old_enabled.len() != new_enabled.len() {
        return Ok(ConfigDiff::FullRestart(
            "Enabled inbound count changed".into(),
        ));
    }

    let mut old_by_tag: HashMap<&str, &InboundConfig> = HashMap::with_capacity(old_enabled.len());
    for ib in &old_enabled {
        old_by_tag.insert(ib.tag.as_str(), *ib);
    }

    let mut ops = Vec::new();

    for new_ib in &new_enabled {
        let Some(old_ib) = old_by_tag.get(new_ib.tag.as_str()).copied() else {
            return Ok(ConfigDiff::FullRestart(format!(
                "Inbound tag '{}' added or renamed",
                new_ib.tag
            )));
        };

        if old_ib.id != new_ib.id
            || old_ib.protocol != new_ib.protocol
            || old_ib.port != new_ib.port
            || old_ib.listen != new_ib.listen
            || old_ib.sniffing != new_ib.sniffing
            || old_ib.stream != new_ib.stream
            || old_ib.routing != new_ib.routing
            || !port_shares_structurally_equal(&old_ib.port_shares, &new_ib.port_shares)
            || strip_user_keys(&old_ib.settings) != strip_user_keys(&new_ib.settings)
        {
            return Ok(ConfigDiff::FullRestart(format!(
                "Inbound '{}' structural configuration changed",
                new_ib.tag
            )));
        }

        let old_users = normalize_users_for_inbound(old_ib)?;
        let new_users = normalize_users_for_inbound(new_ib)?;

        if old_users == new_users {
            continue;
        }

        if !is_xray_protocol(new_ib.protocol) {
            return Ok(ConfigDiff::FullRestart(format!(
                "Non-Xray inbound '{}' users changed",
                new_ib.tag
            )));
        }

        // Single-user non-2022 Shadowsocks cannot use multi-user AddUser/RemoveUser without client table
        if new_ib.protocol == Protocol::Shadowsocks {
            let method = {
                let m = str_or_empty(&new_ib.settings, "method");
                if m.is_empty() {
                    "aes-256-gcm"
                } else {
                    m
                }
            };
            if (old_users.len() <= 1 || new_users.len() <= 1) && !is_ss2022(method) {
                return Ok(ConfigDiff::FullRestart(
                    "Legacy single-user Shadowsocks requires full config reload".into(),
                ));
            }
        }

        let old_map: HashMap<&str, &CoreUser> =
            old_users.iter().map(|u| (u.email.as_str(), u)).collect();
        let new_map: HashMap<&str, &CoreUser> =
            new_users.iter().map(|u| (u.email.as_str(), u)).collect();

        let method = {
            let m = str_or_empty(&new_ib.settings, "method");
            if m.is_empty() {
                "aes-256-gcm".to_string()
            } else {
                m.to_string()
            }
        };

        // 1. Removed or modified users -> RemoveUser first
        for (email, old_u) in &old_map {
            match new_map.get(email) {
                None => {
                    ops.push(UserOp::Remove {
                        inbound_tag: new_ib.tag.clone(),
                        email: (*email).to_string(),
                    });
                }
                Some(new_u) if *new_u != *old_u => {
                    ops.push(UserOp::Remove {
                        inbound_tag: new_ib.tag.clone(),
                        email: (*email).to_string(),
                    });
                    ops.push(UserOp::Add {
                        inbound_tag: new_ib.tag.clone(),
                        protocol: new_ib.protocol,
                        user: (*new_u).clone(),
                        cipher_or_method: method.clone(),
                    });
                }
                Some(_) => {}
            }
        }

        // 2. Newly added users -> AddUser
        for new_u in &new_users {
            if !old_map.contains_key(new_u.email.as_str()) {
                ops.push(UserOp::Add {
                    inbound_tag: new_ib.tag.clone(),
                    protocol: new_ib.protocol,
                    user: new_u.clone(),
                    cipher_or_method: method.clone(),
                });
            }
        }
    }

    if ops.is_empty() {
        Ok(ConfigDiff::NoChange)
    } else {
        Ok(ConfigDiff::HotReload(ops))
    }
}

fn strip_user_keys(v: &Value) -> Value {
    let Some(obj) = v.as_object() else {
        return v.clone();
    };
    let mut cleaned: Map<String, Value> = obj.clone();
    for k in [
        "clients", "users", "id", "uuid", "email", "name", "username", "auth",
    ] {
        cleaned.remove(k);
    }
    Value::Object(cleaned)
}

fn port_shares_structurally_equal(
    a: &[mypanel_proto::PortShareConfig],
    b: &[mypanel_proto::PortShareConfig],
) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (pa, pb) in a.iter().zip(b.iter()) {
        if pa.id != pb.id
            || pa.tag != pb.tag
            || pa.protocol != pb.protocol
            || pa.host != pb.host
            || pa.path != pb.path
            || pa.enable != pb.enable
            || pa.stream != pb.stream
            || pa.users != pb.users
            || strip_user_keys(&pa.settings) != strip_user_keys(&pb.settings)
        {
            return false;
        }
    }
    true
}

fn normalize_users_for_inbound(inbound: &InboundConfig) -> Result<Vec<CoreUser>, String> {
    let mut users = extract_protocol_users(&inbound.users, &inbound.settings, false)?;
    let default_flow = {
        let f = str_or_empty(&inbound.settings, "flow");
        if f.is_empty() {
            str_or_empty(&inbound.stream, "flow")
        } else {
            f
        }
    };
    let vless_route = {
        let r = str_or_empty(&inbound.settings, "vlessRouteId");
        if r.is_empty() {
            str_or_empty(&inbound.settings, "vlessRoute")
        } else {
            r
        }
    };
    let method = {
        let m = str_or_empty(&inbound.settings, "method");
        if m.is_empty() {
            "aes-256-gcm"
        } else {
            m
        }
    };

    for u in &mut users {
        if inbound.protocol == Protocol::Vless {
            if u.flow.is_empty() && !default_flow.is_empty() {
                u.flow = default_flow.to_string();
            }
            if !vless_route.is_empty() && !u.id.is_empty() {
                u.id = apply_vless_route(&u.id, vless_route).map_err(|e| e.to_string())?;
            }
        } else if inbound.protocol == Protocol::Shadowsocks && is_ss2022(method) && !u.password.is_empty() {
            u.password = derive_ss2022_psk(&u.password, method);
        }
    }
    Ok(users)
}

// Prost Protobuf Messages for Xray HandlerService & StatsService

/// `xray.common.serial.TypedMessage`
#[derive(Clone, PartialEq, Message)]
pub struct TypedMessage {
    #[prost(string, tag = "1")]
    pub r#type: String,
    #[prost(bytes = "vec", tag = "2")]
    pub value: Vec<u8>,
}

/// `xray.common.protocol.User`
#[derive(Clone, PartialEq, Message)]
pub struct ProtoUser {
    #[prost(uint32, tag = "1")]
    pub level: u32,
    #[prost(string, tag = "2")]
    pub email: String,
    #[prost(message, optional, tag = "3")]
    pub account: Option<TypedMessage>,
}

/// `xray.proxy.vless.Account`
#[derive(Clone, PartialEq, Message)]
pub struct VlessAccount {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "2")]
    pub flow: String,
    #[prost(string, tag = "3")]
    pub encryption: String,
}

/// `xray.proxy.vmess.Account`
#[derive(Clone, PartialEq, Message)]
pub struct VmessAccount {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(uint32, tag = "2")]
    pub alter_id: u32,
}

/// `xray.proxy.trojan.Account`
#[derive(Clone, PartialEq, Message)]
pub struct TrojanAccount {
    #[prost(string, tag = "1")]
    pub password: String,
}

/// `xray.proxy.shadowsocks.Account`
#[derive(Clone, PartialEq, Message)]
pub struct ShadowsocksAccount {
    #[prost(string, tag = "1")]
    pub password: String,
    #[prost(int32, tag = "2")]
    pub cipher_type: i32,
}

/// `xray. proxy.shadowsocks_2022.User` / `Account`
#[derive(Clone, PartialEq, Message)]
pub struct Shadowsocks2022Account {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(string, tag = "2")]
    pub email: String,
    #[prost(int32, tag = "3")]
    pub level: i32,
}

/// `xray.app.proxyman.command.AddUserOperation`
#[derive(Clone, PartialEq, Message)]
pub struct AddUserOperation {
    #[prost(message, optional, tag = "1")]
    pub user: Option<ProtoUser>,
}

/// `xray.app.proxyman.command.RemoveUserOperation`
#[derive(Clone, PartialEq, Message)]
pub struct RemoveUserOperation {
    #[prost(string, tag = "1")]
    pub email: String,
}

/// `xray.app.proxyman.command.AlterInboundRequest`
#[derive(Clone, PartialEq, Message)]
pub struct AlterInboundRequest {
    #[prost(string, tag = "1")]
    pub tag: String,
    #[prost(message, optional, tag = "2")]
    pub operation: Option<TypedMessage>,
}

/// `xray.app.proxyman.command.AlterInboundResponse`
#[derive(Clone, PartialEq, Message)]
pub struct AlterInboundResponse {}

/// `xray.app.stats.command.QueryStatsRequest`
#[derive(Clone, PartialEq, Message)]
pub struct QueryStatsRequest {
    #[prost(string, tag = "1")]
    pub pattern: String,
    #[prost(bool, tag = "2")]
    pub reset: bool,
}

/// `xray.app.stats.command.Stat`
#[derive(Clone, PartialEq, Message)]
pub struct StatEntry {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(int64, tag = "2")]
    pub value: i64,
}

/// `xray.app.stats.command.QueryStatsResponse`
#[derive(Clone, PartialEq, Message)]
pub struct QueryStatsResponse {
    #[prost(message, repeated, tag = "1")]
    pub stat: Vec<StatEntry>,
}

/// `xray.app.stats.command.GetAllOnlineUsersRequest`
#[derive(Clone, PartialEq, Message)]
pub struct GetAllOnlineUsersRequest {}

/// `xray.app.stats.command.GetAllOnlineUsersResponse`
#[derive(Clone, PartialEq, Message)]
pub struct GetAllOnlineUsersResponse {
    #[prost(string, repeated, tag = "1")]
    pub users: Vec<String>,
}

/// Encodes a [`UserOp`] into a protobuf [`AlterInboundRequest`] for `xray.app.proxyman.command.HandlerService/AlterInbound`.
pub fn build_alter_inbound_request(op: &UserOp) -> AlterInboundRequest {
    match op {
        UserOp::Remove { inbound_tag, email } => {
            let rem = RemoveUserOperation {
                email: email.clone(),
            };
            AlterInboundRequest {
                tag: inbound_tag.clone(),
                operation: Some(TypedMessage {
                    r#type: "xray.app.proxyman.command.RemoveUserOperation".into(),
                    value: rem.encode_to_vec(),
                }),
            }
        }
        UserOp::Add {
            inbound_tag,
            protocol,
            user,
            cipher_or_method,
        } => {
            let account = match protocol {
                Protocol::Vless => {
                    let acc = VlessAccount {
                        id: user.id.clone(),
                        flow: user.flow.clone(),
                        encryption: "none".into(),
                    };
                    TypedMessage {
                        r#type: "xray.proxy.vless.Account".into(),
                        value: acc.encode_to_vec(),
                    }
                }
                Protocol::Vmess => {
                    let acc = VmessAccount {
                        id: user.id.clone(),
                        alter_id: 0,
                    };
                    TypedMessage {
                        r#type: "xray.proxy.vmess.Account".into(),
                        value: acc.encode_to_vec(),
                    }
                }
                Protocol::Trojan => {
                    let acc = TrojanAccount {
                        password: user.password.clone(),
                    };
                    TypedMessage {
                        r#type: "xray.proxy.trojan.Account".into(),
                        value: acc.encode_to_vec(),
                    }
                }
                Protocol::Shadowsocks => {
                    if is_ss2022(cipher_or_method) {
                        let acc = Shadowsocks2022Account {
                            key: user.password.clone(),
                            email: user.email.clone(),
                            level: user.level as i32,
                        };
                        TypedMessage {
                            r#type: "xray.proxy.shadowsocks_2022.Account".into(),
                            value: acc.encode_to_vec(),
                        }
                    } else {
                        let ct = match cipher_or_method.as_str() {
                            "aes-128-gcm" => 5,
                            "aes-256-gcm" => 6,
                            "chacha20-poly1305" | "chacha20-ietf-poly1305" => 7,
                            _ => 6,
                        };
                        let acc = ShadowsocksAccount {
                            password: user.password.clone(),
                            cipher_type: ct,
                        };
                        TypedMessage {
                            r#type: "xray.proxy.shadowsocks.Account".into(),
                            value: acc.encode_to_vec(),
                        }
                    }
                }
                _ => TypedMessage {
                    r#type: String::new(),
                    value: Vec::new(),
                },
            };

            let add = AddUserOperation {
                user: Some(ProtoUser {
                    level: user.level,
                    email: user.email.clone(),
                    account: Some(account),
                }),
            };

            AlterInboundRequest {
                tag: inbound_tag.clone(),
                operation: Some(TypedMessage {
                    r#type: "xray.app.proxyman.command.AddUserOperation".into(),
                    value: add.encode_to_vec(),
                }),
            }
        }
    }
}

// Generic Prost Codec for direct Tonic unary calls without generated `.proto` files
#[derive(Debug, Clone, Default)]
struct ProstCodec<T, U>(std::marker::PhantomData<(T, U)>);

#[derive(Debug, Clone, Default)]
struct ProstEncoder<T>(std::marker::PhantomData<T>);

#[derive(Debug, Clone, Default)]
struct ProstDecoder<U>(std::marker::PhantomData<U>);

impl<T, U> Codec for ProstCodec<T, U>
where
    T: Message + Send + 'static,
    U: Message + Default + Send + 'static,
{
    type Encode = T;
    type Decode = U;
    type Encoder = ProstEncoder<T>;
    type Decoder = ProstDecoder<U>;

    fn encoder(&mut self) -> Self::Encoder {
        ProstEncoder(std::marker::PhantomData)
    }

    fn decoder(&mut self) -> Self::Decoder {
        ProstDecoder(std::marker::PhantomData)
    }
}

impl<T: Message> Encoder for ProstEncoder<T> {
    type Item = T;
    type Error = tonic::Status;

    fn encode(&mut self, item: Self::Item, buf: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        item.encode(buf)
            .map_err(|e| tonic::Status::internal(format!("prost encode error: {e}")))
    }
}

impl<U: Message + Default> Decoder for ProstDecoder<U> {
    type Item = U;
    type Error = tonic::Status;

    fn decode(&mut self, buf: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        let item = U::decode(buf)
            .map_err(|e| tonic::Status::internal(format!("prost decode error: {e}")))?;
        Ok(Some(item))
    }
}

/// Live Tonic gRPC client for Xray `HandlerService` and `StatsService` (`127.0.0.1:<api_port>`).
#[derive(Debug, Clone)]
pub struct TonicXrayClient {
    pub api_port: u16,
    pub xray_bin: PathBuf,
}

impl TonicXrayClient {
    pub fn new(api_port: u16, xray_bin: impl AsRef<Path>) -> Self {
        Self {
            api_port,
            xray_bin: xray_bin.as_ref().to_path_buf(),
        }
    }

    pub fn endpoint_uri(&self) -> String {
        format!("http://127.0.0.1:{}", self.api_port)
    }

    /// Calls `xray.app.proxyman.command.HandlerService/AlterInbound` via Tonic gRPC.
    pub async fn alter_inbound(&self, op: &UserOp) -> Result<(), String> {
        let endpoint = Endpoint::from_shared(self.endpoint_uri())
            .map_err(|e| format!("Invalid Xray gRPC URI: {e}"))?
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(5));

        let channel = endpoint
            .connect()
            .await
            .map_err(|e| format!("Xray gRPC connect failed: {e}"))?;

        let mut grpc = Grpc::new(channel);
        grpc.ready()
            .await
            .map_err(|e| format!("Xray gRPC service not ready: {e}"))?;

        let req = tonic::Request::new(build_alter_inbound_request(op));
        let path = tonic::codegen::http::uri::PathAndQuery::from_static(
            "/xray.app.proxyman.command.HandlerService/AlterInbound",
        );
        let codec: ProstCodec<AlterInboundRequest, AlterInboundResponse> = ProstCodec::default();
        grpc.unary(req, path, codec)
            .await
            .map_err(|e| format!("AlterInbound RPC failed: {e}"))?;
        Ok(())
    }

    /// Queries Xray `StatsService/QueryStats` over gRPC (`pattern: "user>>>"`, `reset: false`).
    pub async fn query_user_stats(&self, reset: bool) -> Result<Vec<StatEntry>, String> {
        let endpoint = Endpoint::from_shared(self.endpoint_uri())
            .map_err(|e| format!("Invalid Xray gRPC URI: {e}"))?
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(5));

        let channel = endpoint
            .connect()
            .await
            .map_err(|e| format!("Xray gRPC connect failed: {e}"))?;

        let mut grpc = Grpc::new(channel);
        grpc.ready()
            .await
            .map_err(|e| format!("Xray gRPC service not ready: {e}"))?;

        let req = tonic::Request::new(QueryStatsRequest {
            pattern: "user>>>".into(),
            reset,
        });
        let path = tonic::codegen::http::uri::PathAndQuery::from_static(
            "/xray.app.stats.command.StatsService/QueryStats",
        );
        let codec: ProstCodec<QueryStatsRequest, QueryStatsResponse> = ProstCodec::default();
        let resp = grpc
            .unary(req, path, codec)
            .await
            .map_err(|e| format!("QueryStats RPC failed: {e}"))?;
        Ok(resp.into_inner().stat)
    }

    /// Queries `StatsService/GetAllOnlineUsers` (Xray 26.x).
    pub async fn get_all_online_users(&self) -> Result<Vec<String>, String> {
        let endpoint = Endpoint::from_shared(self.endpoint_uri())
            .map_err(|e| format!("Invalid Xray gRPC URI: {e}"))?
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(5));

        let channel = endpoint
            .connect()
            .await
            .map_err(|e| format!("Xray gRPC connect failed: {e}"))?;

        let mut grpc = Grpc::new(channel);
        grpc.ready()
            .await
            .map_err(|e| format!("Xray gRPC service not ready: {e}"))?;

        let req = tonic::Request::new(GetAllOnlineUsersRequest {});
        let path = tonic::codegen::http::uri::PathAndQuery::from_static(
            "/xray.app.stats.command.StatsService/GetAllOnlineUsers",
        );
        let codec: ProstCodec<GetAllOnlineUsersRequest, GetAllOnlineUsersResponse> =
            ProstCodec::default();
        let resp = grpc
            .unary(req, path, codec)
            .await
            .map_err(|e| format!("GetAllOnlineUsers RPC failed: {e}"))?;
        Ok(resp.into_inner().users)
    }
}

impl XrayHandlerClient for TonicXrayClient {
    fn apply_ops<'a>(
        &'a self,
        ops: &'a [UserOp],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            for op in ops {
                self.alter_inbound(op).await?;
            }
            Ok(())
        })
    }
}
