原则
你应该尽可能减少指定类型，多使用模板类型。

数据结构
Client（程序外）-(1)-> Player (struct) -(2)-> (Client to Server)Processor -(3)-> Room -(4)-> RoomProvider
RoomProvider -(5)-> 同一个Room -(6)-> (Server to Client) Processor -(7) -> 同一个Player -(8)-> 同一个Client

(1)(2)(7)(8) 数据类型为Byte。
(3)(4) 数据类型为 ctos::Message
(5)(6) 数据类型为 stoc::Message

Player上有两个额外的入口，按数据类型区分，不区分来源：
  - 接收 Bytes。
  - 接收 stoc::Message。
Room上有一个额外的入口，接收 ctos::Message，投递从Processor而来的消息，经(4)发送给RoomProvider。
Room上有一个函数，可以向属于这个Room的指定Player发送 stoc::Message。
Room上有一个函数，接收 stoc::Message，向所有属于这个Room的Player广播。

Processor，核心部分
Processor是一个纯函数的管道。持有按message_type分组的handler注册表（组内按occasion+priority排序）和一个共享State。它对每条消息构造Bundle走handler链，合并后的Response决定是否继续、是否替换消息体。两个方向共有两个实例（同类型），注入了不同的handler注册表。

Handler本身是纯函数：输入Bundle、输出Bundle，不牵扯Actor逻辑。Actor的发送端handle（mpsc::UnboundedSender）通过State.data注入，需要时FromRequest取出即可。

Player创建时，还没有Room，它创建完之后，可以被Processor取走进行连接。连接的过程还包括向Room投递。
