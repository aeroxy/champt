var socket = io.connect('http://localhost');
$(document).ready(function(){
	function drawMap(latitude,longitude){
		var gLatLng = new google.maps.LatLng(latitude,longitude);
		var mapOptions = {
			center: gLatLng,
			zoom: 18,
			mapTypeId: google.maps.MapTypeId.ROADMAP,
			draggable: false,
			zoomControl: false,
			scrollwheel: false,
			disableDoubleClickZoom: true,
			disableDefaultUI: true,
		};
		var map = new google.maps.Map(document.getElementById("map-canvas"),mapOptions);
		var nameLatLng = new google.maps.LatLng(latitude - 0.00004, longitude);
		var userLocation = {
			path: google.maps.SymbolPath.CIRCLE,
			strokeColor: "#4c8f2a",
			strokeWeight: 2,
			fillColor: "#a2e346",
			fillOpacity: 1,
			scale: 6,
		};
		var drawCircle = new google.maps.Marker({
			position: gLatLng,
			icon: userLocation,
			map: map,
		});
		var mapUserName = new MapLabel({
			text: 'Anonymous User',
			position: nameLatLng,
			map: map,
			fontSize: 14,
			fontColor: "#ffffff",
			strokeWeight: 4,
			strokeColor: "4c8f2a",
			fontFamily: "Avenir",
		});
	};
	function drawUser(latitude,longitude){
		var gLatLng = new google.maps.LatLng(latitude,longitude);
		var nameLatLng = new google.maps.LatLng(latitude - 0.00004, longitude);
		var userLocation = {
			path: google.maps.SymbolPath.CIRCLE,
			strokeColor: "#4c8f2a",
			strokeWeight: 2,
			fillColor: "#a2e346",
			fillOpacity: 1,
			scale: 6,
		};
		var drawCircle = new google.maps.Marker({
			position: gLatLng,
			icon: userLocation,
			map: map,
		});
		var mapUserName = new MapLabel({
			text: 'Anonymous User',
			position: nameLatLng,
			map: map,
			fontSize: 14,
			fontColor: "#ffffff",
			strokeWeight: 4,
			strokeColor: "4c8f2a",
			fontFamily: "Avenir",
		});
	};
	var drawEverything = navigator.geolocation.watchPosition(function(position){
		var latitude = position.coords.latitude;
		var longitude = position.coords.longitude;
		socket.emit('addToMap',latitude,longitude);
		socket.on('meOnMap',function(latitude,longitude){
			drawMap(latitude,longitude);
			socket.on('otherOnMap',function(latitude,longitude){
			});
		});
	});
});