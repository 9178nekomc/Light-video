#!/usr/bin/env python
# delete_video.py - 后台删除视频脚本

from app import app
from database import db, Video, User, Like, Favorite, Comment, Message, Notification
import os
import sys

def list_all_videos():
    """列出所有视频"""
    with app.app_context():
        videos = Video.query.all()
        if not videos:
            print("数据库中没有视频")
            return
        
        print("\n=== 所有视频列表 ===")
        for v in videos:
            author = User.query.get(v.user_id)
            author_name = author.username if author else "未知用户"
            print(f"ID: {v.id} | 标题: {v.title} | 作者: {author_name} | 播放: {v.views} | 点赞: {v.like_count}")

def delete_video_by_id(video_id):
    """根据ID删除视频"""
    with app.app_context():
        video = Video.query.get(video_id)
        if not video:
            print(f"错误: ID为 {video_id} 的视频不存在")
            return False
        
        print(f"\n找到视频: {video.title}")
        print(f"作者ID: {video.user_id}")
        print(f"视频文件: {video.video_file}")
        print(f"封面文件: {video.cover_image}")
        
        # 确认删除
        confirm = input(f"\n确定要删除视频 '{video.title}' 吗？(yes/no): ")
        if confirm.lower() != 'yes':
            print("已取消删除")
            return False
        
        try:
            # 删除视频文件
            video_path = os.path.join('uploads/videos', video.video_file)
            if os.path.exists(video_path):
                os.remove(video_path)
                print(f"✓ 已删除视频文件: {video.video_file}")
            
            # 删除封面文件
            if video.cover_image != 'default_cover.jpg':
                cover_path = os.path.join('uploads/covers', video.cover_image)
                if os.path.exists(cover_path):
                    os.remove(cover_path)
                    print(f"✓ 已删除封面文件: {video.cover_image}")
            
            # 删除相关通知
            Notification.query.filter(
                (Notification.type.in_(['like', 'favorite', 'comment'])) &
                (Notification.related_id == video_id)
            ).delete()
            print(f"✓ 已删除相关通知")
            
            # 删除相关消息
            Message.query.filter_by(video_id=video_id).delete()
            print(f"✓ 已删除相关消息")
            
            # 删除视频（会级联删除点赞、收藏、评论）
            db.session.delete(video)
            db.session.commit()
            print(f"✓ 视频 '{video.title}' 已从数据库删除")
            
            return True
            
        except Exception as e:
            db.session.rollback()
            print(f"✗ 删除失败: {str(e)}")
            return False

def delete_videos_by_user(username):
    """删除指定用户的所有视频"""
    with app.app_context():
        user = User.query.filter_by(username=username).first()
        if not user:
            print(f"错误: 用户 '{username}' 不存在")
            return
        
        videos = Video.query.filter_by(user_id=user.id).all()
        if not videos:
            print(f"用户 '{username}' 没有视频")
            return
        
        print(f"\n找到用户 '{username}' 的 {len(videos)} 个视频:")
        for v in videos:
            print(f"  ID: {v.id} - {v.title}")
        
        confirm = input(f"\n确定要删除该用户的所有视频吗？(yes/no): ")
        if confirm.lower() != 'yes':
            print("已取消删除")
            return
        
        success_count = 0
        for video in videos:
            try:
                # 删除文件
                video_path = os.path.join('uploads/videos', video.video_file)
                if os.path.exists(video_path):
                    os.remove(video_path)
                
                if video.cover_image != 'default_cover.jpg':
                    cover_path = os.path.join('uploads/covers', video.cover_image)
                    if os.path.exists(cover_path):
                        os.remove(cover_path)
                
                # 删除相关通知
                Notification.query.filter_by(related_id=video.id).delete()
                
                # 删除相关消息
                Message.query.filter_by(video_id=video.id).delete()
                
                # 删除视频
                db.session.delete(video)
                success_count += 1
                
            except Exception as e:
                print(f"删除视频 {video.id} 时出错: {str(e)}")
        
        db.session.commit()
        print(f"完成！成功删除 {success_count}/{len(videos)} 个视频")

def main():
    print("=" * 50)
    print("视频删除工具")
    print("=" * 50)
    print("1. 列出所有视频")
    print("2. 根据ID删除视频")
    print("3. 根据用户名删除该用户的所有视频")
    print("4. 退出")
    print("-" * 50)
    
    choice = input("请选择操作 (1-4): ")
    
    if choice == '1':
        list_all_videos()
    elif choice == '2':
        list_all_videos()
        try:
            video_id = int(input("\n请输入要删除的视频ID: "))
            delete_video_by_id(video_id)
        except ValueError:
            print("请输入有效的数字ID")
    elif choice == '3':
        username = input("请输入用户名: ")
        delete_videos_by_user(username)
    elif choice == '4':
        print("再见！")
        sys.exit(0)
    else:
        print("无效的选择")

if __name__ == "__main__":
    while True:
        main()
        input("\n按回车键继续...")
        print("\n" * 2)