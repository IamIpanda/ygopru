Single Duel是一个有副作用的processor。
输入：ctos::Message
状态：&mut SingleDuel
输出：Response<stoc::Message>
continue：无事发生
replace：返回此消息。
发给其他人的消息，用SingleDuel发回吗？.
发给其他人的消息，就是直接投递，只有发回给本人的消息作为response。

SingleDuel是个Actor……吗？
我同时接收来自多个来源的消息，显然是需要落锁的。
否，不需要落锁，它是一个mspc的收信箱。

CorePlayer是核心来看的player，它只包含对核心的含义。
NetPlayer是我们加入游戏的时候，看到的Player位置。

只有Response一条数据需要发给核心。所以在Response的处理里直接给何新处理即可。
