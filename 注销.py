#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""
账号注销工具 - 可以查看所有用户并选择注销
"""

import os
import sys
#import msvcrt
from app import app
from database import db, User, Video, Like, Favorite, Comment, Friendship, Message, Notification, Danmaku

def clear_screen():
    """清屏"""
    os.system('cls' if os.name == 'nt' else 'clear')

def print_header(title):
    """打印标题"""
    print("=" * 60)
    print(f"{title:^60}")
    print("=" * 60)

def list_all_users():
    """列出所有用户"""
    with app.app_context():
        users = User.query.order_by(User.id).all()
        
        if not users:
            print("\n📭 数据库中没有用户")
            return []
        
        print_header("用户列表")
        print(f"{'ID':^6} {'用户名':^20} {'邮箱':^25} {'注册时间':^20} {'视频数':^8}")
        print("-" * 80)
        
        for user in users:
            video_count = Video.query.filter_by(user_id=user.id).count()
            created_at = user.created_at.strftime('%Y-%m-%d') if user.created_at else '未知'
            print(f"{user.id:^6} {user.username:^20} {user.email:^25} {created_at:^20} {video_count:^8}")
        
        print("-" * 80)
        print(f"总计: {len(users)} 个用户")
        return users

def get_user_stats(user_id):
    """获取用户统计数据"""
    with app.app_context():
        video_count = Video.query.filter_by(user_id=user_id).count()
        like_count = Like.query.filter_by(user_id=user_id).count()
        favorite_count = Favorite.query.filter_by(user_id=user_id).count()
        comment_count = Comment.query.filter_by(user_id=user_id).count()
        friend_count = Friendship.query.filter(
            ((Friendship.user_id == user_id) | (Friendship.friend_id == user_id)) &
            (Friendship.status == 'accepted')
        ).count()
        message_count = Message.query.filter(
            (Message.sender_id == user_id) | (Message.receiver_id == user_id)
        ).count()
        danmaku_count = Danmaku.query.filter_by(user_id=user_id).count()
        
        return {
            'videos': video_count,
            'likes': like_count,
            'favorites': favorite_count,
            'comments': comment_count,
            'friends': friend_count,
            'messages': message_count,
            'danmakus': danmaku_count
        }

def show_user_details(user_id):
    """显示用户详细信息"""
    with app.app_context():
        user = User.query.get(user_id)
        if not user:
            print(f"错误: 用户ID {user_id} 不存在")
            return False
        
        stats = get_user_stats(user_id)
        
        print_header(f"用户详情 - {user.username}")
        print(f"用户ID: {user.id}")
        print(f"用户名: {user.username}")
        print(f"邮箱: {user.email}")
        print(f"注册时间: {user.created_at.strftime('%Y-%m-%d %H:%M:%S') if user.created_at else '未知'}")
        print(f"个人简介: {user.bio or '无'}")
        print(f"头像: {user.avatar}")
        print("-" * 60)
        print("📊 统计数据:")
        print(f"  上传视频: {stats['videos']} 个")
        print(f"  点赞: {stats['likes']} 次")
        print(f"  收藏: {stats['favorites']} 次")
        print(f"  评论: {stats['comments']} 条")
        print(f"  好友: {stats['friends']} 个")
        print(f"  消息: {stats['messages']} 条")
        print(f"  弹幕: {stats['danmakus']} 条")
        print("-" * 60)
        
        return True

def delete_user_files(user):
    """删除用户相关的所有文件"""
    deleted_files = 0
    
    # 删除用户上传的视频文件
    videos = Video.query.filter_by(user_id=user.id).all()
    for video in videos:
        try:
            # 删除视频文件
            video_path = os.path.join('uploads/videos', video.video_file)
            if os.path.exists(video_path):
                os.remove(video_path)
                deleted_files += 1
                print(f"  ✓ 已删除视频文件: {video.video_file}")
            
            # 删除封面文件
            if video.cover_image and video.cover_image != 'default_cover.jpg':
                cover_path = os.path.join('uploads/covers', video.cover_image)
                if os.path.exists(cover_path):
                    os.remove(cover_path)
                    deleted_files += 1
                    print(f"  ✓ 已删除封面文件: {video.cover_image}")
        except Exception as e:
            print(f"  ✗ 删除文件失败: {e}")
    
    # 删除用户头像（如果不是默认头像）
    if user.avatar and user.avatar != 'default_avatar.png':
        avatar_path = os.path.join('uploads/avatars', user.avatar)
        if os.path.exists(avatar_path):
            try:
                os.remove(avatar_path)
                deleted_files += 1
                print(f"  ✓ 已删除头像文件: {user.avatar}")
            except Exception as e:
                print(f"  ✗ 删除头像失败: {e}")
    
    return deleted_files

def delete_user_complete(user_id):
    """完全删除用户及其所有相关数据"""
    with app.app_context():
        user = User.query.get(user_id)
        if not user:
            print(f"错误: 用户ID {user_id} 不存在")
            return False
        
        username = user.username
        email = user.email
        
        print(f"\n🗑️  正在注销账号: {username} (ID: {user_id})")
        print("-" * 60)
        
        # 获取统计数据
        stats = get_user_stats(user_id)
        print(f"将删除的数据:")
        print(f"  • 视频: {stats['videos']} 个")
        print(f"  • 点赞: {stats['likes']} 条")
        print(f"  • 收藏: {stats['favorites']} 条")
        print(f"  • 评论: {stats['comments']} 条")
        print(f"  • 好友关系: {stats['friends']} 个")
        print(f"  • 消息: {stats['messages']} 条")
        print(f"  • 弹幕: {stats['danmakus']} 条")
        
        # 删除用户相关文件
        print("\n📁 删除相关文件:")
        file_count = delete_user_files(user)
        
        # 删除用户（数据库会通过级联删除自动删除所有关联数据）
        try:
            db.session.delete(user)
            db.session.commit()
            print(f"\n✅ 账号注销成功！")
            print(f"  • 已从数据库删除用户: {username}")
            print(f"  • 已删除 {file_count} 个文件")
            return True
        except Exception as e:
            db.session.rollback()
            print(f"\n❌ 注销失败: {e}")
            return False

def select_user_interactive():
    """交互式选择用户"""
    users = list_all_users()
    if not users:
        input("\n按回车键返回...")
        return None
    
    while True:
        print("\n" + "=" * 60)
        print("请输入要操作的用户ID (输入 0 返回):")
        try:
            choice = input("> ").strip()
            if choice == '0':
                return None
            
            user_id = int(choice)
            user = next((u for u in users if u.id == user_id), None)
            if user:
                return user
            else:
                print(f"错误: 不存在ID为 {user_id} 的用户")
        except ValueError:
            print("错误: 请输入有效的数字ID")

def main():
    while True:
        clear_screen()
        print_header("账号注销工具")
        print("1. 查看所有用户")
        print("2. 查看用户详情")
        print("3. 注销账号")
        print("4. 批量注销（按条件）")
        print("5. 退出")
        print("=" * 60)
        
        choice = input("请选择操作 (1-5): ").strip()
        
        if choice == '1':
            clear_screen()
            list_all_users()
            input("\n按回车键继续...")
        
        elif choice == '2':
            clear_screen()
            users = list_all_users()
            if users:
                user = select_user_interactive()
                if user:
                    clear_screen()
                    show_user_details(user.id)
                    input("\n按回车键继续...")
        
        elif choice == '3':
            clear_screen()
            users = list_all_users()
            if users:
                user = select_user_interactive()
                if user:
                    clear_screen()
                    show_user_details(user.id)
                    print("\n" + "=" * 60)
                    print("⚠️  警告: 注销账号将永久删除所有数据！")
                    print("   • 所有上传的视频")
                    print("   • 所有点赞、收藏、评论")
                    print("   • 所有好友关系和消息")
                    print("   • 所有弹幕")
                    print("=" * 60)
                    
                    confirm = input(f"\n确认注销账号 '{user.username}'？\n输入用户名确认: ").strip()
                    
                    if confirm == user.username:
                        confirm2 = input("最后确认，输入 'YES' 执行注销: ").strip()
                        if confirm2 == 'YES':
                            if delete_user_complete(user.id):
                                input("\n按回车键继续...")
                        else:
                            print("\n❌ 注销已取消")
                            input("按回车键继续...")
                    else:
                        print("\n❌ 用户名不匹配，注销已取消")
                        input("按回车键继续...")
        
        elif choice == '4':
            clear_screen()
            print_header("批量注销")
            print("1. 注销所有未上传视频的用户")
            print("2. 注销所有注册超过30天且无视频的用户")
            print("3. 注销指定邮箱后缀的用户")
            print("4. 返回")
            print("-" * 60)
            
            sub_choice = input("请选择 (1-4): ").strip()
            
            if sub_choice == '1':
                with app.app_context():
                    users = User.query.all()
                    to_delete = []
                    for user in users:
                        video_count = Video.query.filter_by(user_id=user.id).count()
                        if video_count == 0:
                            to_delete.append(user)
                    
                    if not to_delete:
                        print("\n没有符合条件的用户")
                    else:
                        print(f"\n找到 {len(to_delete)} 个无视频的用户:")
                        for user in to_delete:
                            print(f"  • {user.username} (ID: {user.id})")
                        
                        confirm = input(f"\n确认注销这 {len(to_delete)} 个用户？(yes/no): ")
                        if confirm.lower() == 'yes':
                            success = 0
                            for user in to_delete:
                                if delete_user_complete(user.id):
                                    success += 1
                            print(f"\n完成！成功注销 {success}/{len(to_delete)} 个用户")
                    
                    input("\n按回车键继续...")
            
            elif sub_choice == '2':
                from datetime import datetime, timedelta
                cutoff_date = datetime.utcnow() - timedelta(days=30)
                
                with app.app_context():
                    users = User.query.filter(User.created_at < cutoff_date).all()
                    to_delete = []
                    for user in users:
                        video_count = Video.query.filter_by(user_id=user.id).count()
                        if video_count == 0:
                            to_delete.append(user)
                    
                    if not to_delete:
                        print("\n没有符合条件的用户")
                    else:
                        print(f"\n找到 {len(to_delete)} 个注册超过30天且无视频的用户:")
                        for user in to_delete:
                            created = user.created_at.strftime('%Y-%m-%d') if user.created_at else '未知'
                            print(f"  • {user.username} (注册: {created})")
                        
                        confirm = input(f"\n确认注销这 {len(to_delete)} 个用户？(yes/no): ")
                        if confirm.lower() == 'yes':
                            success = 0
                            for user in to_delete:
                                if delete_user_complete(user.id):
                                    success += 1
                            print(f"\n完成！成功注销 {success}/{len(to_delete)} 个用户")
                    
                    input("\n按回车键继续...")
            
            elif sub_choice == '3':
                suffix = input("请输入邮箱后缀 (如 @example.com): ").strip()
                if suffix:
                    with app.app_context():
                        users = User.query.filter(User.email.like(f'%{suffix}')).all()
                        
                        if not users:
                            print(f"\n没有找到邮箱以 '{suffix}' 结尾的用户")
                        else:
                            print(f"\n找到 {len(users)} 个用户:")
                            for user in users:
                                print(f"  • {user.username} - {user.email}")
                            
                            confirm = input(f"\n确认注销这 {len(users)} 个用户？(yes/no): ")
                            if confirm.lower() == 'yes':
                                success = 0
                                for user in users:
                                    if delete_user_complete(user.id):
                                        success += 1
                                print(f"\n完成！成功注销 {success}/{len(users)} 个用户")
                        
                        input("\n按回车键继续...")
        
        elif choice == '5':
            print("\n再见！")
            break

def quick_delete_by_username(username):
    """快速通过用户名删除用户（命令行快捷方式）"""
    with app.app_context():
        user = User.query.filter_by(username=username).first()
        if not user:
            print(f"错误: 用户 '{username}' 不存在")
            return False
        
        print(f"找到用户: {user.username} (ID: {user.id})")
        confirm = input(f"确认注销该用户？(yes/no): ")
        if confirm.lower() == 'yes':
            return delete_user_complete(user.id)
        return False

if __name__ == "__main__":
    # 命令行参数支持
    if len(sys.argv) > 1:
        if sys.argv[1] == '--help' or sys.argv[1] == '-h':
            print("使用方法:")
            print("  python delete_user.py              # 交互式菜单")
            print("  python delete_user.py list         # 列出所有用户")
            print("  python delete_user.py <用户名>     # 直接删除指定用户")
            sys.exit(0)
        elif sys.argv[1] == 'list':
            with app.app_context():
                list_all_users()
            sys.exit(0)
        else:
            # 尝试删除指定用户名的用户
            quick_delete_by_username(sys.argv[1])
            sys.exit(0)
    
    try:
        main()
    except KeyboardInterrupt:
        print("\n\n程序已退出")