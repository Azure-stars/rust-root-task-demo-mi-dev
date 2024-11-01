# Rel4 宏内核 IPC 设计文档

## 项目背景

- 在 rel4 微内核上运行一个甚至多个宏内核
- 复用 ArceOS 原有的组件
- 基座是 [rust-sel4](https://github.com/seL4/rust-sel4)，底层使用的 OS 初始为 sel4，之后会逐步替换为 rel4



## 设计思路

在微内核角度下，我们将任务分为了三类，并在当前的项目目录下实现了对应的 demo。

- 内核任务：负责接收来自用户程序的 syscall，并且将其转发到对应的设备任务上；对应的 demo 为 [kernel-thread](../crates/kernel-thread)
- 用户程序任务：负责运行待测的用户态二进制文件，并将其调用的 syscall 转化为 IPC 和内核任务进行通信；对应的 demo 为 [test-thread](../crates/test-thread)。
- 设备任务：如网络任务等，通过网络栈与各类网络设备交互，并且向上提供接口；对应的 demo 为 [net-thread](../crates/net-thread)。

一个关键的问题是如何设计不同类型任务之间对应的接口，从而在保持组件通用性的同时能够适配微内核的 IPC 的特点（不适合传递过大信息）。对于用户程序任务和内核任务，已经有较为完善的 syscall 接口的定义。因此我们需要考虑的有以下两部分：

- 内核任务和设备任务之间的通信接口的设计：适用于宏内核
- 用户程序任务和设备任务之间的通信接口设计：适用于 Unikernel、将来可能存在的 bypass kernel module 等

为了能够更好地应用 ArceOS 等其他已有的组件，我们提出了如下的兼容思路：复用原先的设备组件，如网络设备、文件系统设备等，其对外提供的接口不变，为函数调用的形式。当我们希望能将其作为用户组件运行的时候，就**为其添加一个 IPC 模块**，负责接收外部的请求，并且进行设备函数的调用，将响应结果返回回去。因此通过决定是否使用 IPC 接管，可以方便地决定该模块是否在用户态运行。



## 设计实现

为了能够更好的应用 ArceOS 的组件，我们以 net-thread 为例，复用了 ArceOS 的 [smoltcp 网络栈](https://github.com/arceos-org/arceos/tree/main/modules/axnet/src/smoltcp_impl)，并添加了对应的 IPC 接口，从而能让其与内核任务进行通信。并且我们还定义了一套专门用于网络通信的 IPC 接口，当网络设备任务和内核任务都实现了对应的 IPC 接口之后，就可以进行通信。



为了保证该组件的复用性，我们希望该组件仅提供必要的功能，不包含额外的 IPC 或者兼容 posix 接口的实现。因此我们将 IPC 和真正 syscall 功能的实现从复用组件当中划分出来，详细的设计实现如下：

- common：
  - lib/NetRequsetabel：定义了网络部分相关的 IPC 接口，接口的定义是按照 Unikernel Smoltcp 网络栈向外暴露的接口进行设计的，默认第一个参数为 socket id 用于识别进行当前网络操作的套接字，其他的参数的意义可以参见：[接口构建](https://github.com/Azure-stars/rust-root-task-demo-mi-dev/blob/docs/crates/common/src/lib.rs#L293)

- net-thread：

  - smoltcp/：绝大部份移植了 ArceOS 的 Smoltcp 网络栈，但由于简便起步暂时只移植了 TCP 部分，这部分的组件可以给其他内核复用
  - ipc/：实现了网络设备接收 IPC 的接口。**另外为了维护微内核的设备功能，我们将 socket 的状态维护放在了 IPC 模块中，而不是内核任务中，从而让内核任务仅仅是作为一个中转设备的功能，通过 socket id 来请求、修改 socket 状态。**

- kernel-thread：

  - syscall/net/ipc：在内核端实现的和网络设备通信的 IPC 接口，将对设备的函数调用转化为 IPC
  - syscall/net/net_impl：syscall 语义中与网络相关的实现，目前还较为简单

- test-thread：用户程序任务，通过 vsyscall handler 来调用 syscall 实现（vsyscall_handler 本身是为了应对 syscall 转发使用的，但是目前的 test-thread 是我们自己编写的程序，会在 kernel-thread 中被启动起来，因此调用 vsyscall_handler 就只是一个普通的 function call）

  > 二进制文件（而不是我们自己写的源码程序）如何将 syscall 重定向到 vsyscall_handler：用户态程序原先的 syscall 流程是通过一个 vsyscall 的特殊入口。杨金博学长通过改动 gcc 源码，将所有涉及到的 syscall 入口改动为了一个特殊的 vsyscall 符号，然后再通过 shim-comp 文件中 [修改 vsyscall 符号指向内容](https://github.com/rel4team/rust-root-task-demo-mi-dev/blob/docs/crates/shim-comp/src/main.rs#L75)，从而接管所有的 syscall，并且将其转化为 IPC。但是这个 gcc 环境目前没有公开，自行修改复现也比较麻烦。目前采用自行编写源码的方式，直接调用 vsyscall 而不是让 gcc 进行编译，从而能够完成本地测试。

这里我们使用了 IPC 传递 capability 的形式来进行跨地址空间的内存访问，因此要求 IPC 提供的接口不能有超过 1 个的内存访问需求，否则就需要进一步简化接口。

## 测试方式

### 网络模块的本地测试

目前网络模块的初始化代码如下：

```rust
#[export_name = "_start"]
fn main(ipc_buffer: IPCBuffer) -> sel4::Result<!> {
    static LOGGER: sel4_logging::Logger = sel4_logging::LoggerBuilder::const_default()
        .write(|s| sel4::debug_print!("{}", s))
        .level_filter(log::LevelFilter::Trace)
        .fmt(fmt_with_module)
        .build();
    LOGGER.set().unwrap();
    set_ipc_buffer(ipc_buffer);
    debug_println!("[Net Thread] Net-Thread");

    let virtio_net = VirtIoNetDev::<VirtIoHalImpl, MmioTransport, 32>::try_new(unsafe {
        MmioTransport::new(NonNull::new(VIRTIO_NET_ADDR as *mut VirtIOHeader).unwrap()).unwrap()
    })
    .expect("failed to create net driver");

    debug_println!(
        "[Net Thread] Net device mac address: {:?}",
        virtio_net.mac_address().0
    );

    smoltcp_impl::init(virtio_net);
  
  	// Modify here
    ipc::run_ipc();

    unreachable!()
}
```



我们提供了两个测试程序，分别测试作为 client 和 server 的表现，相关代码位于 [test_net](../crates/net-thread/src/smoltcp_impl/test.rs)

- client

  测试代码如下：

  ```rust
  fn test_client() {
      const REQUEST: &str = "\
      GET / HTTP/1.1\r\n\
      Host: ident.me\r\n\
      Accept: */*\r\n\
      \r\n";
  
      let tcp_socket = TcpSocket::new();
      tcp_socket
          .connect(SocketAddr::new(
              IpAddr::V4(Ipv4Addr::new(49, 12, 234, 183)),
              80,
          ))
          .unwrap();
      let request_buf = REQUEST.as_bytes();
      tcp_socket.send(request_buf).unwrap();
      let mut response_buf = [0; 256];
      let cnt = tcp_socket.recv(&mut response_buf).unwrap();
      let response = core::str::from_utf8(&response_buf[..cnt]).unwrap();
      debug_println!("response: {:?}", response);
  }
  ```

  运行方式：在上文提到的网络模块初始化代码中，将 ipc 入口修改为 test_client 的入口，即修改为：

  ```rust
  #[export_name = "_start"]
  fn main(ipc_buffer: IPCBuffer) -> sel4::Result<!> {
  		xxx
    	// Modify here
      // ipc::run_ipc();
  		smoltcp_impl::test::test_client();
      unreachable!()
  }
  ```

  然后运行

  ```shell
  $ make -C docker/ exec ARCH=aarch64
  ```

  

- Server

  测试代码同上，将 IPC 入口修改为 test_server 入口即可。在运行内核之后，我们会在主机的 6379 号端口上启动 qemu，并且监听来自外部的网络请求。此时可以另开一个终端，运行如下指令：

  ```shell
  $ echo "Hello, World!" | nc localhost 6378
  ```

  即可观测到微内核的 server 接收到了请求，并且做出了响应。



### 宏内核测试

目前的宏内核用户程序进行了对应的网络测试，详见 [test-thread](../crates/test-thread)。在这里他通过 vsyscall handler 新建了一个 socket，并且按照 syscall 的语义，开始监听指定的端口并响应对应的请求。即我们利用宏内核的 syscall 建立了一个 server。在启动之后我们可以像网络模块的本地测试一样，通过运行如下指令：

```rust
$ echo "Hello, World!" | nc localhost 6378
```

来对宏内核的 Server 进行访问测试。